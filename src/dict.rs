//! DICT protocol (RFC 2229).
//!
//! Dict URLs look like `dict://server/d:word[:database]` (define),
//! `dict://server/m:word[:database[:strategy]]` (match), or just
//! `dict://server/word` (define against any database).

use crate::error::{Error, Result};
use crate::url::Url;

/// Connect, issue the DICT request encoded in `url.path`, and return the
/// human-readable text response.
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme("dict not yet implemented".into()))
}
