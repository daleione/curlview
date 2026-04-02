use colored::*;

use crate::client::HttpResponse;
use crate::metrics::DisplayMetrics;

pub fn print_connection_info(m: &DisplayMetrics, show_ip: bool) {
    if show_ip {
        println!(
            "{} {}:{}  ⇄  {}:{}",
            "IP Info:".blue(),
            m.local_ip,
            m.local_port,
            m.remote_ip,
            m.remote_port
        );
    }
}

pub fn print_headers(resp: &HttpResponse) {
    let status_line = format!("{:?} {}", resp.version, resp.status);
    println!("{}", status_line.green());

    for (name, value) in resp.headers.iter() {
        let val = value.to_str().unwrap_or("<binary>");
        println!("{}:{}", name.as_str().bright_black(), val.cyan());
    }
}

pub fn print_body(body: &[u8], show_body: bool) {
    if !show_body {
        return;
    }

    let text = String::from_utf8_lossy(body);
    if text.len() > 1024 {
        println!("{}{}", &text[..1024], "...".cyan());
    } else {
        println!("{}", text);
    }
}

pub fn print_speed(m: &DisplayMetrics) {
    println!(
        "{} {:.1} KiB/s, {} {:.1} KiB/s",
        "Download:".bright_green(),
        m.speed_download / 1024.0,
        "Upload:".bright_green(),
        m.speed_upload / 1024.0
    );
}

pub fn print_timing_chart(m: &DisplayMetrics, https: bool) {
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
