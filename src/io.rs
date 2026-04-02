use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

pin_project! {
    #[project = MaybeHttpsProj]
    pub enum MaybeHttpsStream {
        Plain { #[pin] inner: TcpStream },
        Tls   { #[pin] inner: TlsStream<TcpStream> },
    }
}

impl AsyncRead for MaybeHttpsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.project() {
            MaybeHttpsProj::Plain { inner } => inner.poll_read(cx, buf),
            MaybeHttpsProj::Tls { inner } => inner.poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeHttpsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.project() {
            MaybeHttpsProj::Plain { inner } => inner.poll_write(cx, buf),
            MaybeHttpsProj::Tls { inner } => inner.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.project() {
            MaybeHttpsProj::Plain { inner } => inner.poll_flush(cx),
            MaybeHttpsProj::Tls { inner } => inner.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.project() {
            MaybeHttpsProj::Plain { inner } => inner.poll_shutdown(cx),
            MaybeHttpsProj::Tls { inner } => inner.poll_shutdown(cx),
        }
    }
}
