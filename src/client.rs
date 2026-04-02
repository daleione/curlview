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
use crate::metrics::TimingMetrics;

pub struct HttpResponse {
    pub status: http::StatusCode,
    pub version: http::Version,
    pub headers: http::HeaderMap,
    pub body: Vec<u8>,
}

pub struct RequestConfig {
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            timeout: Duration::from_secs(10),
        }
    }
}

pub async fn timed_request(
    url_str: &str,
    config: &RequestConfig,
) -> Result<(TimingMetrics, HttpResponse), Box<dyn std::error::Error>> {
    let url = Url::parse(url_str)?;
    let host = url
        .host_str()
        .ok_or("URL missing host")?
        .to_string();
    let port = url.port_or_known_default().ok_or("Unknown port")?;
    let is_https = url.scheme() == "https";
    let epoch = Instant::now();

    // ── Phase 1: DNS Resolve ──
    let resolver = TokioResolver::builder_tokio()?.build();
    let lookup = resolver.lookup_ip(host.as_str()).await?;
    let remote_ip = lookup
        .iter()
        .next()
        .ok_or("DNS lookup returned no addresses")?;
    let t_namelookup = epoch.elapsed();

    // ── Phase 2: TCP Connect ──
    let remote_addr = SocketAddr::new(remote_ip, port);
    let tcp_stream = TcpStream::connect(remote_addr).await?;
    let local_addr = tcp_stream.local_addr()?;
    let t_connect = epoch.elapsed();

    // ── Phase 3: TLS Handshake (HTTPS only) ──
    let (stream, t_appconnect) = if is_https {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls_config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(tls_config));
        let server_name = ServerName::try_from(host.clone())?;
        let tls_stream = connector.connect(server_name, tcp_stream).await?;
        let t = epoch.elapsed();
        (MaybeHttpsStream::Tls { inner: tls_stream }, t)
    } else {
        (MaybeHttpsStream::Plain { inner: tcp_stream }, t_connect)
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
    let body_bytes = response.into_body().collect().await?.to_bytes().to_vec();
    let t_total = epoch.elapsed();

    let metrics = TimingMetrics {
        t_namelookup,
        t_connect,
        t_appconnect,
        t_starttransfer,
        t_total,
        local_addr,
        remote_addr,
        body_size: body_bytes.len(),
    };

    let http_response = HttpResponse {
        status,
        version,
        headers,
        body: body_bytes,
    };

    Ok((metrics, http_response))
}
