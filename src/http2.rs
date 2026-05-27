//! HTTP/2 support (RFC 9113), with HPACK header compression (RFC 7541).
//!
//! HTTP/2 reuses the `https://` URL scheme; the version is selected at
//! connect time, typically via ALPN ("h2"). This module exposes a backend
//! that can serve [`crate::Request`] over a TLS connection negotiated with
//! ALPN, returning a [`crate::Response`] just like HTTP/1.1.

use crate::error::{Error, Result};
use crate::{Request, Response};

/// Send a single request/response over a fresh HTTP/2 connection.
/// (Connection pooling and multiplexing come later.)
pub fn send(_req: Request) -> Result<Response> {
    Err(Error::UnsupportedScheme("http/2 not yet implemented".into()))
}
