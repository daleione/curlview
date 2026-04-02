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
    url: String,
    show_body: bool,
    show_ip: bool,
    show_speed: bool,
    debug: bool,
    follow_redirects: bool,
    req: RequestConfig,
}

fn parse_args() -> Result<Config, String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_help();
        std::process::exit(0);
    }

    let url = if args[1].contains("://") {
        args[1].clone()
    } else {
        format!("http://{}", args[1])
    };

    let mut show_body = false;
    let mut show_ip = true;
    let mut show_speed = false;
    let mut debug = false;
    let mut follow_redirects = true;
    let mut req = RequestConfig::default();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            // ── Request options ──
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
            "--max-time" | "--timeout" | "-t" => {
                i += 1;
                let secs: u64 = args
                    .get(i)
                    .ok_or("--max-time requires a value")?
                    .parse()
                    .map_err(|_| "Invalid timeout value")?;
                req.timeout = Duration::from_secs(secs);
            }

            // ── Redirect ──
            "-L" | "--location" => {
                follow_redirects = true;
            }
            "--no-follow" => {
                follow_redirects = false;
            }

            // ── Display options ──
            "-b" | "--show-body" => {
                show_body = true;
            }
            "-s" | "--show-speed" => {
                show_speed = true;
            }
            "--no-ip" => {
                show_ip = false;
            }
            "--debug" => {
                debug = true;
            }

            other => {
                return Err(format!("Unknown option: {}", other));
            }
        }
        i += 1;
    }

    req.follow_redirects = follow_redirects;

    Ok(Config {
        url,
        show_body,
        show_ip,
        show_speed,
        debug,
        follow_redirects,
        req,
    })
}

fn main() {
    let config = match parse_args() {
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
            config.req.method,
            config.url,
            config.req.timeout.as_secs(),
            config.follow_redirects,
        );
        for (k, v) in &config.req.headers {
            eprintln!("  {}: {}", k.bright_black(), v.cyan());
        }
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    rt.block_on(async {
        let result = tokio::time::timeout(
            config.req.timeout,
            client::timed_request(&config.url, &config.req),
        )
        .await;

        match result {
            Ok(Ok(r)) => {
                let dm = DisplayMetrics::from(&r.timing);
                let is_https = r.final_url.starts_with("https://");

                print_redirect_chain(
                    &r.redirect_chain,
                    &r.final_url,
                    r.response.status.as_u16(),
                );

                print_connection_section(
                    &dm,
                    &r.timing.dns_info,
                    r.timing.tls_info.as_ref(),
                    config.show_ip,
                );

                let speed_ref = if config.show_speed { Some(&dm) } else { None };
                print_response(&r.response, &r.timing.size_info, speed_ref);

                print_body(&r.response.body, config.show_body);

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
                    config.req.timeout.as_secs()
                );
                std::process::exit(1);
            }
        }
    });
}

fn print_help() {
    println!(
        "{} curlview URL [OPTIONS]\n",
        "Usage:".green().bold()
    );

    println!("{}", "Request Options:".blue().bold());
    print_opt("-X, --request METHOD", "HTTP method (GET, POST, PUT, DELETE, ...)");
    print_opt("-H, --header \"K: V\"", "Add request header");
    print_opt("-d, --data DATA", "Request body (auto-sets POST if GET)");
    print_opt("-t, --timeout SECONDS", "Request timeout (default: 10)");

    println!("\n{}", "Redirect Options:".blue().bold());
    print_opt("-L, --location", "Follow redirects (default: on)");
    print_opt("--no-follow", "Disable redirect following");

    println!("\n{}", "Display Options:".blue().bold());
    print_opt("-b, --show-body", "Show response body");
    print_opt("-s, --show-speed", "Show download speed");
    print_opt("--no-ip", "Hide connection IP info");
    print_opt("--debug", "Print debug info");

    println!();
    print_opt("-h, --help", "Show this help");
}

fn print_opt(flag: &str, desc: &str) {
    println!("  {:<24}{}", flag.cyan(), desc);
}
