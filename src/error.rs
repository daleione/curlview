use std::fmt;

/// A request failure tagged with the phase it occurred in, so we can show the
/// user a readable message instead of a raw library error string.
#[derive(Debug)]
pub enum RequestError {
    InvalidUrl(String),
    Dns { host: String, detail: String },
    Connect { addr: String, detail: String },
    Tls { host: String, detail: String },
    Http(String),
}

impl RequestError {
    /// An optional one-line hint suggesting what to check next.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            RequestError::Dns { detail, .. } => Some(
                if detail.contains("no records found") || detail.contains("NXDomain") {
                    "No DNS record for this domain. Check the spelling, \
                     or connect to its network/VPN if it's internal."
                } else {
                    "DNS lookup failed — check your network connection and DNS settings."
                },
            ),
            RequestError::Connect { .. } => Some(
                "Nothing responded on that address. \
                 Check the host and port, or a firewall blocking it.",
            ),
            RequestError::Tls { .. } => Some(
                "The server's certificate may be expired or untrusted, \
                 or its TLS version unsupported.",
            ),
            _ => None,
        }
    }

    /// The underlying low-level detail, shown only in --debug mode.
    pub fn detail(&self) -> &str {
        match self {
            RequestError::InvalidUrl(d)
            | RequestError::Dns { detail: d, .. }
            | RequestError::Connect { detail: d, .. }
            | RequestError::Tls { detail: d, .. }
            | RequestError::Http(d) => d,
        }
    }
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequestError::InvalidUrl(d) => write!(f, "Invalid URL: {}", d),
            RequestError::Dns { host, .. } => write!(f, "Could not resolve host: {}", host),
            RequestError::Connect { addr, .. } => write!(f, "Could not connect to {}", addr),
            RequestError::Tls { host, .. } => write!(f, "TLS handshake failed with {}", host),
            RequestError::Http(d) => write!(f, "HTTP request failed: {}", d),
        }
    }
}

impl std::error::Error for RequestError {}
