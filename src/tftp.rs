//! TFTP support (RFC 1350, plus RFC 2347 option extension, RFC 2348 blksize,
//! RFC 2349 timeout/tsize).
//!
//! TFTP runs over UDP, default port 69. URL: `tftp://host/path`. Default
//! operation is a read (RRQ) of `url.path` in octet mode, reassembling
//! 512-byte (or negotiated) blocks until a short block signals end.

use crate::error::{Error, Result};
use crate::url::Url;

/// RRQ the file at `url.path` and return the reassembled bytes.
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme("tftp not yet implemented".into()))
}
