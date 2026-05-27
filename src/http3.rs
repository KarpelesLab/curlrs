//! HTTP/3 support (RFC 9114), with QPACK (RFC 9204) over QUIC (RFC 9000).
//!
//! QUIC transport: use [`purecrypto::quic`] (already a dependency). QPACK is
//! the HTTP/3 analogue of HPACK. HTTP/3 also reuses `https://`; selection is
//! via Alt-Svc or explicit caller preference, not the URL scheme.

use crate::error::{Error, Result};
use crate::{Request, Response};

/// Send a single request/response over a fresh HTTP/3 (QUIC) connection.
pub fn send(_req: Request) -> Result<Response> {
    Err(Error::UnsupportedScheme("http/3 not yet implemented".into()))
}
