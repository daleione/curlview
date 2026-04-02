use colored::*;

use crate::client::HttpResponse;
use crate::metrics::{DnsInfo, DisplayMetrics, RedirectHop, SizeInfo, TlsInfo};

// ── Section dividers ──

fn separator(title: &str) {
    println!();
    let line = "─".repeat(50);
    println!("── {} {}", title.blue().bold(), line.bright_black());
}

// ── Color helpers ──

fn color_ms(ms: f64) -> ColoredString {
    let text = format!("{:.2}ms", ms);
    if ms < 100.0 {
        text.green()
    } else if ms < 500.0 {
        text.yellow()
    } else {
        text.red()
    }
}

fn color_phase_ms(ms: u64) -> ColoredString {
    let text = format!("{}ms", ms);
    if ms < 100 {
        text.green()
    } else if ms < 500 {
        text.yellow()
    } else {
        text.red()
    }
}

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

// ── Connection section: IP + DNS + TLS ──

pub fn print_connection_section(
    m: &DisplayMetrics,
    dns: &DnsInfo,
    tls: Option<&TlsInfo>,
    show_ip: bool,
) {
    separator("Connection");

    if show_ip {
        println!(
            "  {:>10}  {}",
            "Remote:".bright_black(),
            format!("{}:{}", m.remote_ip, m.remote_port).cyan(),
        );
        println!(
            "  {:>10}  {}",
            "Local:".bright_black(),
            format!("{}:{}", m.local_ip, m.local_port).cyan(),
        );
    }

    if dns.resolved_ips.len() > 1 {
        let ips: Vec<String> = dns
            .resolved_ips
            .iter()
            .map(|ip| {
                let kind = if ip.is_ipv4() { "A" } else { "AAAA" };
                format!("{} ({})", ip, kind)
            })
            .collect();
        println!("  {:>10}  {}", "DNS:".bright_black(), ips.join(", ").cyan());
    }

    if let Some(info) = tls {
        println!(
            "  {:>10}  {}",
            "TLS:".bright_black(),
            format!("{} / {}", info.protocol_version, info.cipher_suite).cyan(),
        );
        if !info.cert_subject.is_empty() {
            println!("  {:>10}  {}", "Subject:".bright_black(), info.cert_subject.cyan());
        }
        if !info.cert_issuer.is_empty() {
            println!("  {:>10}  {}", "Issuer:".bright_black(), info.cert_issuer.cyan());
        }
        if let (Some(not_after), Some(days)) = (&info.cert_not_after, info.cert_days_remaining) {
            let expiry = format!("{} ({} days remaining)", not_after, days);
            let colored_expiry = if days < 30 {
                expiry.red()
            } else if days < 90 {
                expiry.yellow()
            } else {
                expiry.green()
            };
            println!("  {:>10}  {}", "Expires:".bright_black(), colored_expiry);
        }
    }
}

// ── Response headers ──

pub fn print_response(resp: &HttpResponse, size: &SizeInfo, speed: Option<&DisplayMetrics>) {
    separator("Response");

    let status_line = format!("{:?} {}", resp.version, resp.status);
    println!("  {}", status_line.green());

    for (name, value) in resp.headers.iter() {
        let val = value.to_str().unwrap_or("<binary>");
        println!("  {}: {}", name.as_str().bright_black(), val.cyan());
    }

    // Size summary as part of response
    println!();
    let headers_part = format!(
        "{}: {}",
        "Headers".bright_black(),
        format_size(size.headers_size).cyan()
    );

    let body_part = match (&size.content_encoding, size.content_length) {
        (Some(encoding), Some(wire_size)) if wire_size != size.body_size && size.body_size > 0 => {
            let ratio = (1.0 - wire_size as f64 / size.body_size as f64) * 100.0;
            format!(
                "{}: {} → {} ({}, {:.1}% saved)",
                "Body".bright_black(),
                format_size(wire_size).yellow(),
                format_size(size.body_size).cyan(),
                encoding.bright_black(),
                ratio,
            )
        }
        (Some(encoding), _) => {
            format!(
                "{}: {} ({})",
                "Body".bright_black(),
                format_size(size.body_size).cyan(),
                encoding.bright_black(),
            )
        }
        _ => {
            format!(
                "{}: {}",
                "Body".bright_black(),
                format_size(size.body_size).cyan(),
            )
        }
    };

    let total_part = format!(
        "{}: {}",
        "Total".bright_black(),
        format_size(size.headers_size + size.body_size).cyan(),
    );

    match speed {
        Some(m) => println!(
            "  {}  {}  {}  {}: {:.1} KiB/s",
            headers_part,
            body_part,
            total_part,
            "Speed".bright_black(),
            m.speed_download / 1024.0,
        ),
        None => println!("  {}  {}  {}", headers_part, body_part, total_part),
    }
}

// ── Response body ──

pub fn print_body(body: &[u8], show_body: bool) {
    if !show_body || body.is_empty() {
        return;
    }

    separator("Body");
    let text = String::from_utf8_lossy(body);
    if text.len() > 1024 {
        println!("  {}{}", &text[..1024], "...".cyan());
    } else {
        for line in text.lines() {
            println!("  {}", line);
        }
    }
}

// ── Timing chart ──

pub fn print_timing_chart(m: &DisplayMetrics, https: bool) {
    separator("Timing");

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
            color_phase_ms(dns),
            color_phase_ms(connect),
            color_phase_ms(ssl),
            color_phase_ms(server),
            color_phase_ms(transfer),
            color_ms(m.time_namelookup * 1000.0),
            color_ms(m.time_connect * 1000.0),
            color_ms(m.time_pretransfer * 1000.0),
            color_ms(m.time_starttransfer * 1000.0),
            color_ms(m.time_total * 1000.0),
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
            color_phase_ms(dns),
            color_phase_ms(connect),
            color_phase_ms(server),
            color_phase_ms(transfer),
            color_ms(m.time_namelookup * 1000.0),
            color_ms(m.time_connect * 1000.0),
            color_ms(m.time_starttransfer * 1000.0),
            color_ms(m.time_total * 1000.0),
        );
    }
}


// ── Redirect chain ──

pub fn print_redirect_chain(chain: &[RedirectHop], final_url: &str, final_status: u16) {
    if chain.is_empty() {
        return;
    }

    separator("Redirect Chain");

    for (i, hop) in chain.iter().enumerate() {
        let hop_time_ms = hop.timing.t_total.as_secs_f64() * 1000.0;
        println!(
            "  {} {} {} {} ({})",
            format!("[{}]", i + 1).bright_black(),
            hop.url.white(),
            "→".bright_black(),
            format!("{}", hop.status).yellow(),
            color_ms(hop_time_ms),
        );
    }

    println!(
        "  {} {} → {}",
        format!("[{}]", chain.len() + 1).bright_black(),
        final_url.white(),
        format!("{}", final_status).green(),
    );

    let total_redirect_ms: f64 = chain
        .iter()
        .map(|h| h.timing.t_total.as_secs_f64() * 1000.0)
        .sum();
    println!(
        "  {:>10}  {}",
        "Total:".bright_black(),
        color_ms(total_redirect_ms),
    );
}
