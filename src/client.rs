use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use hickory_resolver::TokioResolver;
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use url::Url;

use crate::io::MaybeHttpsStream;
use crate::metrics::{DnsInfo, RedirectHop, SizeInfo, TimingMetrics, TlsInfo};

pub struct HttpResponse {
    pub status: http::StatusCode,
    pub version: http::Version,
    pub headers: http::HeaderMap,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub struct RequestConfig {
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
    pub follow_redirects: bool,
    pub max_redirects: usize,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout: Duration::from_secs(10),
            follow_redirects: true,
            max_redirects: 10,
        }
    }
}

/// Result of the full request, possibly including redirect hops.
pub struct RequestResult {
    pub final_url: String,
    pub timing: TimingMetrics,
    pub response: HttpResponse,
    pub redirect_chain: Vec<RedirectHop>,
}

/// Execute request with automatic redirect following.
pub async fn timed_request(
    url_str: &str,
    config: &RequestConfig,
) -> Result<RequestResult, Box<dyn std::error::Error>> {
    let mut current_url = url_str.to_string();
    let mut redirect_chain: Vec<RedirectHop> = Vec::new();

    loop {
        let (timing, response) = single_request(&current_url, config).await?;

        if config.follow_redirects
            && response.status.is_redirection()
            && redirect_chain.len() < config.max_redirects
        {
            let location = response
                .headers
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or("Redirect without Location header")?;

            // Resolve relative redirects against current URL
            let next_url = if location.contains("://") {
                location.to_string()
            } else {
                let base = Url::parse(&current_url)?;
                base.join(location)?.to_string()
            };

            redirect_chain.push(RedirectHop {
                url: current_url.clone(),
                status: response.status.as_u16(),
                timing,
            });

            current_url = next_url;
            continue;
        }

        return Ok(RequestResult {
            final_url: current_url,
            timing,
            response,
            redirect_chain,
        });
    }
}

/// Execute a single HTTP request with per-phase timing.
async fn single_request(
    url_str: &str,
    config: &RequestConfig,
) -> Result<(TimingMetrics, HttpResponse), Box<dyn std::error::Error>> {
    let url = Url::parse(url_str)?;
    let host = url.host_str().ok_or("URL missing host")?.to_string();
    let port = url.port_or_known_default().ok_or("Unknown port")?;
    let is_https = url.scheme() == "https";
    let epoch = Instant::now();

    // ── Phase 1: DNS Resolve ──
    let resolver = TokioResolver::builder_tokio()?.build();
    let lookup = resolver.lookup_ip(host.as_str()).await?;
    let all_ips: Vec<std::net::IpAddr> = lookup.iter().collect();
    let remote_ip = *all_ips.first().ok_or("DNS lookup returned no addresses")?;
    let t_namelookup = epoch.elapsed();

    let dns_info = DnsInfo {
        resolved_ips: all_ips,
    };

    // ── Phase 2: TCP Connect ──
    let remote_addr = SocketAddr::new(remote_ip, port);
    let tcp_stream = TcpStream::connect(remote_addr).await?;
    let local_addr = tcp_stream.local_addr()?;
    let t_connect = epoch.elapsed();

    // ── Phase 3: TLS Handshake (HTTPS only) ──
    let (stream, t_appconnect, tls_info) = if is_https {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls_config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(tls_config));
        let server_name = ServerName::try_from(host.clone())?;
        let tls_stream = connector.connect(server_name, tcp_stream).await?;
        let t = epoch.elapsed();

        let info = extract_tls_info(&tls_stream);

        (MaybeHttpsStream::Tls { inner: tls_stream }, t, Some(info))
    } else {
        (
            MaybeHttpsStream::Plain { inner: tcp_stream },
            t_connect,
            None,
        )
    };

    // ── Phase 4 & 5: HTTP Request + Wait for first byte ──
    let io = TokioIo::new(stream);
    let (mut sender, conn) = http1::handshake(io).await?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            eprintln!("Connection error: {}", e);
        }
    });

    let path = if let Some(q) = url.query() {
        format!("{}?{}", url.path(), q)
    } else {
        url.path().to_string()
    };

    let method: http::Method = config.method.parse()?;
    let mut builder = http::Request::builder()
        .method(method)
        .uri(&path)
        .header("Host", &host)
        .header("User-Agent", "curlview/0.3");

    for (k, v) in &config.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }

    let req_body = match &config.body {
        Some(data) => Full::new(Bytes::from(data.clone())),
        None => Full::new(Bytes::new()),
    };

    let response = sender.send_request(builder.body(req_body)?).await?;
    let t_starttransfer = epoch.elapsed();

    // ── Phase 6: Read Body ──
    let status = response.status();
    let version = response.version();
    let headers = response.headers().clone();

    // Extract size/compression info from headers before consuming body
    let content_encoding = headers
        .get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let content_length = headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());

    // Calculate headers size (approximate: status line + header lines)
    let headers_size = {
        let status_line = format!("{:?} {}\r\n", version, status);
        let header_lines: usize = headers
            .iter()
            .map(|(k, v)| k.as_str().len() + 2 + v.len() + 2) // "key: value\r\n"
            .sum();
        status_line.len() + header_lines + 2 // trailing \r\n
    };

    let body_bytes = response.into_body().collect().await?.to_bytes().to_vec();
    let t_total = epoch.elapsed();

    let size_info = SizeInfo {
        headers_size,
        body_size: body_bytes.len(),
        content_encoding,
        content_length,
    };

    let metrics = TimingMetrics {
        t_namelookup,
        t_connect,
        t_appconnect,
        t_starttransfer,
        t_total,
        local_addr,
        remote_addr,
        body_size: body_bytes.len(),
        tls_info,
        dns_info,
        size_info,
    };

    let http_response = HttpResponse {
        status,
        version,
        headers,
        body: body_bytes,
    };

    Ok((metrics, http_response))
}

/// Extract TLS protocol, cipher suite, and certificate info from a completed handshake.
fn extract_tls_info(tls_stream: &tokio_rustls::client::TlsStream<TcpStream>) -> TlsInfo {
    let (_, conn) = tls_stream.get_ref();

    let protocol_version = conn
        .protocol_version()
        .map(|v| format!("{:?}", v))
        .unwrap_or_else(|| "unknown".to_string());

    let cipher_suite = conn
        .negotiated_cipher_suite()
        .map(|cs| format!("{:?}", cs.suite()))
        .unwrap_or_else(|| "unknown".to_string());

    let mut cert_subject = String::new();
    let mut cert_issuer = String::new();
    let mut cert_not_after: Option<String> = None;
    let mut cert_days_remaining: Option<i64> = None;

    if let Some(certs) = conn.peer_certificates() {
        if let Some(leaf) = certs.first() {
            if let Ok((_, cert)) = x509_parser::parse_x509_certificate(leaf.as_ref()) {
                cert_subject = cert.subject().to_string();
                cert_issuer = cert.issuer().to_string();

                let not_after = cert.validity().not_after.to_datetime();
                cert_not_after = Some(format!("{}", not_after));

                use std::time::SystemTime;
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let expiry = cert.validity().not_after.timestamp();
                cert_days_remaining = Some((expiry - now) / 86400);
            }
        }
    }

    TlsInfo {
        protocol_version,
        cipher_suite,
        cert_subject,
        cert_issuer,
        cert_not_after,
        cert_days_remaining,
    }
}
