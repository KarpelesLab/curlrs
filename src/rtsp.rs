//! RTSP support (RFC 7826, also RFC 2326 for RTSP/1.0).
//!
//! RTSP is HTTP-like over TCP (default port 554) with a request/response
//! shape similar to HTTP/1.x but its own methods (DESCRIBE, SETUP, PLAY, ...).
//! URL: `rtsp://host[:554]/streamid`.

use crate::error::{Error, Result};
use crate::url::Url;

/// Default operation: issue an RTSP `DESCRIBE` and return the response body
/// (typically an SDP document).
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme("rtsp not yet implemented".into()))
}
