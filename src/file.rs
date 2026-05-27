//! `file://` URL support (RFC 8089, formerly RFC 1738).
//!
//! `file:///etc/hosts` reads the local file at `/etc/hosts`. Hosts other
//! than the empty string or `localhost` are rejected per RFC 8089 §2.

use crate::error::{Error, Result};
use crate::url::Url;

/// Read the file at `url.path` and return its contents.
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme("file not yet implemented".into()))
}
