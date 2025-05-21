use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::{Command, Stdio};

use colored::*;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[derive(Deserialize, Serialize, Debug)]
struct CurlMetrics {
    time_namelookup: f64,
    time_connect: f64,
    time_appconnect: f64,
    time_pretransfer: f64,
    time_redirect: f64,
    time_starttransfer: f64,
    time_total: f64,
    speed_download: f64,
    speed_upload: f64,
    remote_ip: String,
    remote_port: u16,
    local_ip: String,
    local_port: u16,
}

#[derive(Debug)]
struct Config {
    show_body: bool,
    show_ip: bool,
    show_speed: bool,
    save_body: bool,
    curl_bin: String,
    debug: bool,
    timeout_secs: u64,
}

impl Config {
    fn from_env() -> Self {
        fn getenv_bool(key: &str, default: bool) -> bool {
            env::var(key)
                .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
                .unwrap_or(default)
        }

        fn getenv_u64(key: &str, default: u64) -> u64 {
            env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        }

        Self {
            show_body: getenv_bool("HTTPSTAT_SHOW_BODY", false),
            show_ip: getenv_bool("HTTPSTAT_SHOW_IP", true),
            show_speed: getenv_bool("HTTPSTAT_SHOW_SPEED", false),
            save_body: getenv_bool("HTTPSTAT_SAVE_BODY", true),
            curl_bin: env::var("HTTPSTAT_CURL_BIN").unwrap_or_else(|_| "curl".to_string()),
            debug: getenv_bool("HTTPSTAT_DEBUG", false),
            timeout_secs: getenv_u64("HTTPSTAT_TIMEOUT", 10),
        }
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_help();
        return Ok(());
    }

    let config = Config::from_env();
    let url = &args[1];
    let extra_args = &args[2..];

    validate_extra_args(extra_args)?;

    let (_header_file, _body_file) = execute_curl(&config, url, extra_args)?;

    Ok(())
}

fn print_help() {
    println!(
        "{}",
        r#"
Usage: httpstat URL [CURL_OPTIONS]
Options:
  -h, --help      Show this help
  --version       Show version

Env Options:
  HTTPSTAT_SHOW_BODY=true       Show response body
  HTTPSTAT_SHOW_IP=false        Disable IP info
  HTTPSTAT_SHOW_SPEED=true      Show speed
  HTTPSTAT_SAVE_BODY=false      Don't save body
  HTTPSTAT_CURL_BIN=/my/curl    Use custom curl
  HTTPSTAT_DEBUG=true           Enable debug log
"#
        .bright_blue()
    );
}

fn validate_extra_args(extra_args: &[String]) -> io::Result<()> {
    let excluded_flags = [
        "-w",
        "-D",
        "-o",
        "-s",
        "--write-out",
        "--dump-header",
        "--output",
        "--silent",
    ];
    if extra_args.iter().any(|arg| {
        excluded_flags
            .iter()
            .any(|&flag| arg == flag || arg.starts_with(&format!("{}=", flag)))
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Error: One or more disallowed flags used",
        ));
    }
    Ok(())
}

fn execute_curl(
    config: &Config,
    url: &str,
    extra_args: &[String],
) -> io::Result<(NamedTempFile, NamedTempFile)> {
    let curl_format = r#"{
        "time_namelookup": %{time_namelookup},
        "time_connect": %{time_connect},
        "time_appconnect": %{time_appconnect},
        "time_pretransfer": %{time_pretransfer},
        "time_redirect": %{time_redirect},
        "time_starttransfer": %{time_starttransfer},
        "time_total": %{time_total},
        "speed_download": %{speed_download},
        "speed_upload": %{speed_upload},
        "remote_ip": "%{remote_ip}",
        "remote_port": %{remote_port},
        "local_ip": "%{local_ip}",
        "local_port": %{local_port}
    }"#;

    let header_file = NamedTempFile::new()?;
    let body_file = NamedTempFile::new()?;

    let mut cmd = Command::new(&config.curl_bin);
    cmd.arg("-w")
        .arg(curl_format)
        .arg("-D")
        .arg(header_file.path())
        .arg("-o")
        .arg(body_file.path())
        .arg("-sS")
        .arg("--max-time")
        .arg(config.timeout_secs.to_string())
        .args(extra_args)
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if config.debug {
        println!("{} {:?}", "Executing:".bright_blue(), cmd);
    }

    let output = cmd.output()?;

    handle_curl_output(config, output, &header_file, &body_file, url)?;
    Ok((header_file, body_file))
}

fn handle_curl_output(
    config: &Config,
    output: std::process::Output,
    header_file: &NamedTempFile,
    body_file: &NamedTempFile,
    url: &str,
) -> io::Result<()> {
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Curl error: {}", String::from_utf8_lossy(&output.stderr)),
        ));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let metrics: CurlMetrics = serde_json::from_str(&stdout_str)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("JSON error: {}", e)))?;

    print_connection_info(&metrics, config.show_ip);
    print_headers(header_file)?;
    handle_response_body(body_file, config.show_body, config.save_body)?;
    print_timing_chart(&metrics, url_is_https(url));

    if config.show_speed {
        println!(
            "{} {:.1} KiB/s, {} {:.1} KiB/s",
            "Download:".bright_green(),
            metrics.speed_download / 1024.0,
            "Upload:".bright_green(),
            metrics.speed_upload / 1024.0
        );
    }

    Ok(())
}

fn handle_response_body(
    body_file: &NamedTempFile,
    show_body: bool,
    save_body: bool,
) -> io::Result<()> {
    let mut body_buf = String::new();
    fs::File::open(body_file.path())?.read_to_string(&mut body_buf)?;

    if show_body {
        let truncated = if body_buf.len() > 1024 {
            format!("{}{}", &body_buf[..1024], "...".cyan())
        } else {
            body_buf.clone()
        };
        println!("{}", truncated);
    } else if save_body {
        println!(
            "{} stored in: {}",
            "Body".green(),
            body_file.path().display()
        );
    }

    if !save_body {
        let _ = fs::remove_file(body_file.path());
    }
    Ok(())
}

fn url_is_https(url: &str) -> bool {
    url.starts_with("https://")
}

fn print_headers(header_file: &NamedTempFile) -> io::Result<()> {
    let mut headers = String::new();
    fs::File::open(header_file.path())?.read_to_string(&mut headers)?;
    for line in headers.lines() {
        if let Some(idx) = line.find(':') {
            println!(
                "{}{}",
                &line[..=idx].bright_black(),
                &line[idx + 1..].cyan()
            );
        } else {
            println!("{}", line.green());
        }
    }
    Ok(())
}

fn print_connection_info(metrics: &CurlMetrics, show_ip: bool) {
    if show_ip {
        println!(
            "{} {}:{}  ⇄  {}:{}",
            "IP Info:".blue(),
            metrics.local_ip,
            metrics.local_port,
            metrics.remote_ip,
            metrics.remote_port
        );
    }
}

fn print_timing_chart(m: &CurlMetrics, https: bool) {
    let dns = (m.time_namelookup * 1000.0) as u64;
    let connect = (m.time_connect * 1000.0) as u64 - dns;
    let ssl = if https {
        (m.time_pretransfer * 1000.0) as u64 - dns - connect
    } else {
        0
    };
    let server = (m.time_starttransfer * 1000.0) as u64 - dns - connect - ssl;
    let transfer = (m.time_total * 1000.0) as u64 - dns - connect - ssl - server;

    if https {
        println!(
            r#"
  DNS Lookup   TCP Connection   TLS Handshake   Server Processing   Content Transfer
[{:^12}|{:^16}|{:^15}|{:^19}|{:^18}]
             |                |               |                   |                  |
   namelookup:{:<8}        |               |                   |                  |
                       connect:{:<8}       |                   |                  |
                                   pretransfer:{:<8}           |                  |
                                                     starttransfer:{:<8}          |
                                                                                total:{:<8}"#,
            format!("{dns}ms").cyan(),
            format!("{connect}ms").cyan(),
            format!("{ssl}ms").cyan(),
            format!("{server}ms").cyan(),
            format!("{transfer}ms").cyan(),
            format!("{:.2}ms", m.time_namelookup * 1000.0).cyan(),
            format!("{:.2}ms", m.time_connect * 1000.0).cyan(),
            format!("{:.2}ms", m.time_pretransfer * 1000.0).cyan(),
            format!("{:.2}ms", m.time_starttransfer * 1000.0).cyan(),
            format!("{:.2}ms", m.time_total * 1000.0).cyan(),
        );
    } else {
        println!(
            r#"
   DNS Lookup   TCP Connection   Server Processing   Content Transfer
[{:^13}|{:^16}|{:^19}|{:^18}]
              |                |                   |                  |
    namelookup:{:<8}        |                   |                  |
                        connect:{:<8}           |                  |
                                      starttransfer:{:<8}          |
                                                                 total:{:<8}"#,
            format!("{dns}ms").cyan(),
            format!("{connect}ms").cyan(),
            format!("{server}ms").cyan(),
            format!("{transfer}ms").cyan(),
            format!("{:.2}ms", m.time_namelookup * 1000.0).cyan(),
            format!("{:.2}ms", m.time_connect * 1000.0).cyan(),
            format!("{:.2}ms", m.time_starttransfer * 1000.0).cyan(),
            format!("{:.2}ms", m.time_total * 1000.0).cyan(),
        );
    }
}

