mod client;
mod display;
mod io;
mod metrics;

use std::env;
use std::time::Duration;

use colored::*;

use client::RequestConfig;
use display::{
    print_body, print_connection_section, print_redirect_chain, print_response, print_timing_chart,
};
use metrics::DisplayMetrics;

#[derive(Debug)]
struct Config {
    show_body: bool,
    show_ip: bool,
    show_speed: bool,
    debug: bool,
    timeout_secs: u64,
    follow_redirects: bool,
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
            debug: getenv_bool("HTTPSTAT_DEBUG", false),
            timeout_secs: getenv_u64("HTTPSTAT_TIMEOUT", 10),
            follow_redirects: getenv_bool("HTTPSTAT_FOLLOW_REDIRECTS", true),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_help();
        return;
    }

    let config = Config::from_env();
    let url = if args[1].contains("://") {
        args[1].clone()
    } else {
        format!("http://{}", args[1])
    };
    let extra_args = &args[2..];

    let req_config = match parse_extra_args(extra_args, &config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {}", "Error:".red(), e);
            std::process::exit(1);
        }
    };

    if config.debug {
        eprintln!(
            "{} {} {} (timeout: {}s, follow_redirects: {})",
            "Request:".bright_blue(),
            req_config.method,
            url,
            req_config.timeout.as_secs(),
            req_config.follow_redirects,
        );
        for (k, v) in &req_config.headers {
            eprintln!("  {}: {}", k.bright_black(), v.cyan());
        }
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    rt.block_on(async {
        let result = tokio::time::timeout(
            req_config.timeout,
            client::timed_request(&url, &req_config),
        )
        .await;

        match result {
            Ok(Ok(r)) => {
                let dm = DisplayMetrics::from(&r.timing);
                let is_https = r.final_url.starts_with("https://");

                // 1. Redirect chain (if any)
                print_redirect_chain(
                    &r.redirect_chain,
                    &r.final_url,
                    r.response.status.as_u16(),
                );

                // 2. Connection: IP + DNS + TLS in one block
                print_connection_section(
                    &dm,
                    &r.timing.dns_info,
                    r.timing.tls_info.as_ref(),
                    config.show_ip,
                );

                // 3. Response headers + size summary
                let speed_ref = if config.show_speed { Some(&dm) } else { None };
                print_response(&r.response, &r.timing.size_info, speed_ref);

                // 4. Response body (optional)
                print_body(&r.response.body, config.show_body);

                // 5. Timing waterfall chart
                print_timing_chart(&dm, is_https);
            }
            Ok(Err(e)) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
            Err(_) => {
                eprintln!(
                    "{} Request timed out after {}s",
                    "Error:".red(),
                    req_config.timeout.as_secs()
                );
                std::process::exit(1);
            }
        }
    });
}

fn parse_extra_args(args: &[String], config: &Config) -> Result<RequestConfig, String> {
    let mut req = RequestConfig {
        timeout: Duration::from_secs(config.timeout_secs),
        follow_redirects: config.follow_redirects,
        ..Default::default()
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-X" | "--request" => {
                i += 1;
                req.method = args
                    .get(i)
                    .ok_or("-X requires a method argument")?
                    .to_uppercase();
            }
            "-H" | "--header" => {
                i += 1;
                let header = args.get(i).ok_or("-H requires a header argument")?;
                let (key, value) = header
                    .split_once(':')
                    .ok_or(format!("Invalid header format: {}", header))?;
                req.headers
                    .push((key.trim().to_string(), value.trim().to_string()));
            }
            "-d" | "--data" => {
                i += 1;
                let data = args.get(i).ok_or("-d requires a data argument")?;
                req.body = Some(data.as_bytes().to_vec());
                if req.method == "GET" {
                    req.method = "POST".to_string();
                }
            }
            "--max-time" => {
                i += 1;
                let secs: u64 = args
                    .get(i)
                    .ok_or("--max-time requires a value")?
                    .parse()
                    .map_err(|_| "Invalid --max-time value")?;
                req.timeout = Duration::from_secs(secs);
            }
            "-L" | "--location" => {
                req.follow_redirects = true;
            }
            "--no-follow" => {
                req.follow_redirects = false;
            }
            other => {
                return Err(format!("Unknown option: {}", other));
            }
        }
        i += 1;
    }

    Ok(req)
}

fn print_help() {
    println!(
        "{}",
        r#"
Usage: curlview URL [OPTIONS]

Options:
  -X, --request METHOD    HTTP method (GET, POST, PUT, DELETE, ...)
  -H, --header "K: V"    Add request header
  -d, --data DATA         Request body (auto-sets POST if GET)
  --max-time SECONDS      Request timeout
  -L, --location          Follow redirects (default: on)
  --no-follow             Disable redirect following
  -h, --help              Show this help

Env Options:
  HTTPSTAT_SHOW_BODY=true            Show response body
  HTTPSTAT_SHOW_IP=false             Disable IP info
  HTTPSTAT_SHOW_SPEED=true           Show speed
  HTTPSTAT_FOLLOW_REDIRECTS=false    Disable redirect following
  HTTPSTAT_DEBUG=true                Enable debug log
  HTTPSTAT_TIMEOUT=10                Request timeout in seconds
"#
        .bright_blue()
    );
}
