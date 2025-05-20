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

fn getenv_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(default)
}

fn getenv_str(key: &str, default: &str) -> String {
    env::var(key).unwrap_or(default.to_string())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_help();
        return Ok(());
    }

    let show_body = getenv_bool("HTTPSTAT_SHOW_BODY", false);
    let show_ip = getenv_bool("HTTPSTAT_SHOW_IP", true);
    let show_speed = getenv_bool("HTTPSTAT_SHOW_SPEED", false);
    let save_body = getenv_bool("HTTPSTAT_SAVE_BODY", true);
    let curl_bin = getenv_str("HTTPSTAT_CURL_BIN", "curl");
    let metrics_only = getenv_bool("HTTPSTAT_METRICS_ONLY", false);
    let debug = getenv_bool("HTTPSTAT_DEBUG", false);

    let url = &args[1];
    let extra_args = &args[2..];

    let excluded_flags = ["-w", "-D", "-o", "-s", "--write-out", "--dump-header", "--output", "--silent"];
    for flag in excluded_flags {
        if extra_args.contains(&flag.to_string()) {
            eprintln!("{}", format!("Error: {} is not allowed", flag).yellow());
            std::process::exit(1);
        }
    }

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

    let mut cmd = Command::new(curl_bin);
    cmd.arg("-w")
        .arg(curl_format)
        .arg("-D")
        .arg(header_file.path())
        .arg("-o")
        .arg(body_file.path())
        .arg("-s")
        .arg("-S")
        .args(extra_args)
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if debug {
        println!("Executing: {:?}", cmd);
    }

    let output = cmd.output()?;

    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr).red());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let metrics: CurlMetrics = serde_json::from_str(&stdout_str)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("JSON parse error: {}", e)))?;

    if show_ip {
        println!(
            "Connected to {}:{} from {}:{}",
            metrics.remote_ip.cyan(),
            metrics.remote_port.to_string().cyan(),
            metrics.local_ip,
            metrics.local_port
        );
        println!();
    }

    if metrics_only {
        println!("{}", serde_json::to_string_pretty(&metrics).unwrap());
        return Ok(());
    }

    // Headers
    let mut header_buf = String::new();
    fs::File::open(header_file.path())?.read_to_string(&mut header_buf)?;
    for line in header_buf.lines() {
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

    println!();

    // Body
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
        println!("{} stored in: {}", "Body".green(), body_file.path().display());
    }

    if !save_body {
        let _ = fs::remove_file(body_file.path());
    }

    // Timing chart
    print_timing_chart(&metrics, url.starts_with("https://"));

    if show_speed {
        println!("speed_download: {:.1} KiB/s, speed_upload: {:.1} KiB/s", metrics.speed_download / 1024.0, metrics.speed_upload / 1024.0 )
    }
    Ok(())
}

fn print_timing_chart(m: &CurlMetrics, https: bool) {
    let dns = (m.time_namelookup * 1000.0) as u64;
    let connect = (m.time_connect * 1000.0) as u64 - dns;
    let ssl = (m.time_pretransfer * 1000.0) as u64 - dns - connect;
    let server = (m.time_starttransfer * 1000.0) as u64 - dns - connect - ssl;
    let transfer = (m.time_total * 1000.0) as u64 - dns - connect - ssl - server;

    if https {
        println!(
            r#"
  DNS Lookup   TCP Connection   TLS Handshake   Server Processing   Content Transfer
[  {:>7}  |    {:>7}    |   {:>7}    |      {:>7}     |     {:>7}     ]
"#,
            format!("{}ms", dns).cyan(),
            format!("{}ms", connect).cyan(),
            format!("{}ms", ssl).cyan(),
            format!("{}ms", server).cyan(),
            format!("{}ms", transfer).cyan(),
        );
    } else {
        println!(
            r#"
  DNS Lookup   TCP Connection   Server Processing   Content Transfer
[  {:>7}  |    {:>7}    |      {:>7}     |     {:>7}     ]
"#,
            format!("{}ms", dns).cyan(),
            format!("{}ms", connect).cyan(),
            format!("{}ms", server).cyan(),
            format!("{}ms", transfer).cyan(),
        );
    }
}

fn print_help() {
    println!(
        "{}",
        r#"
Usage: httpstat URL [CURL_OPTIONS]
Options:
  -h, --help      Show this help.
  --version       Show version.

Env Options:
  HTTPSTAT_SHOW_BODY=true       Show response body
  HTTPSTAT_SHOW_IP=false        Disable IP info
  HTTPSTAT_SHOW_SPEED=true      Show speed
  HTTPSTAT_SAVE_BODY=false      Don't save body
  HTTPSTAT_CURL_BIN=/my/curl    Use custom curl
  HTTPSTAT_DEBUG=true           Enable debug log
"#
    );
}
