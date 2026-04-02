use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// TLS connection details extracted after handshake.
#[derive(Debug, Clone)]
pub struct TlsInfo {
    pub protocol_version: String,
    pub cipher_suite: String,
    pub cert_subject: String,
    pub cert_issuer: String,
    pub cert_not_after: Option<String>,
    pub cert_days_remaining: Option<i64>,
}

/// DNS resolution details.
#[derive(Debug, Clone)]
pub struct DnsInfo {
    pub resolved_ips: Vec<IpAddr>,
}

/// Response size and compression details.
#[derive(Debug, Clone)]
pub struct SizeInfo {
    pub headers_size: usize,
    pub body_size: usize,
    pub content_encoding: Option<String>,
    /// The Content-Length header value (compressed size on wire), if present.
    pub content_length: Option<usize>,
}

/// A single hop in a redirect chain.
#[derive(Debug)]
pub struct RedirectHop {
    pub url: String,
    pub status: u16,
    pub timing: TimingMetrics,
}

#[derive(Debug)]
pub struct TimingMetrics {
    pub t_namelookup: Duration,
    pub t_connect: Duration,
    pub t_appconnect: Duration,
    pub t_starttransfer: Duration,
    pub t_total: Duration,

    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,

    pub body_size: usize,

    pub tls_info: Option<TlsInfo>,
    pub dns_info: DnsInfo,
    pub size_info: SizeInfo,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct DisplayMetrics {
    pub time_namelookup: f64,
    pub time_connect: f64,
    pub time_appconnect: f64,
    pub time_pretransfer: f64,
    pub time_redirect: f64,
    pub time_starttransfer: f64,
    pub time_total: f64,
    pub speed_download: f64,
    pub speed_upload: f64,
    pub remote_ip: String,
    pub remote_port: u16,
    pub local_ip: String,
    pub local_port: u16,
}

impl From<&TimingMetrics> for DisplayMetrics {
    fn from(t: &TimingMetrics) -> Self {
        let total_secs = t.t_total.as_secs_f64();
        Self {
            time_namelookup: t.t_namelookup.as_secs_f64(),
            time_connect: t.t_connect.as_secs_f64(),
            time_appconnect: t.t_appconnect.as_secs_f64(),
            time_pretransfer: t.t_appconnect.as_secs_f64(),
            time_redirect: 0.0,
            time_starttransfer: t.t_starttransfer.as_secs_f64(),
            time_total: total_secs,
            speed_download: if total_secs > 0.0 {
                t.body_size as f64 / total_secs
            } else {
                0.0
            },
            speed_upload: 0.0,
            remote_ip: t.remote_addr.ip().to_string(),
            remote_port: t.remote_addr.port(),
            local_ip: t.local_addr.ip().to_string(),
            local_port: t.local_addr.port(),
        }
    }
}
