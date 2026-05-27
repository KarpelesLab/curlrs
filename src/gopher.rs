//! Gopher and Gopher-over-TLS support (RFC 1436 + the TLS extension).
//!
//! Gopher URLs are `gopher://host[:70]/<type><selector>` where `<type>` is a
//! single character item type (e.g. `1` directory, `0` text file). For TLS
//! use [`crate::tls::connect_over`].

use crate::error::{Error, Result};
use crate::url::Url;

/// Send the selector from `url.path` and read the server's response until
/// the connection is closed (gopher has no length framing).
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme(
        "gopher/gophers not yet implemented".into(),
    ))
}
