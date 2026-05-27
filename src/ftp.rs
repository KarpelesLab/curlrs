//! FTP and FTPS support.
//!
//! Specs: RFC 959 (FTP), RFC 4217 (FTP over TLS / "explicit FTPS"),
//! plus the implicit-FTPS convention of TLS-from-start on port 990.
//!
//! The agent implementing this module is expected to support both `ftp://`
//! (plain) and `ftps://` (implicit TLS on connect). For TLS use the
//! [`crate::tls::connect_over`] helper.

use crate::error::{Error, Result};
use crate::url::Url;

/// Default operation: download the file at `url.path`, or list the directory
/// if the path ends in `/`. Returns the raw bytes.
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme("ftp/ftps not yet implemented".into()))
}
