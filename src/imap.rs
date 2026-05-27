//! IMAP and IMAPS support.
//!
//! Specs: RFC 9051 (IMAP4rev2), RFC 3501 (IMAP4rev1), RFC 8314 (implicit TLS
//! on port 993 for IMAPS), RFC 5092 (`imap:` URL scheme).
//!
//! IMAP URLs look like `imap://user@host/INBOX;UID=42` — the path/parameters
//! select a mailbox and optionally a single message to FETCH. For IMAPS,
//! wrap the TCP stream with [`crate::tls::connect_over`] before LOGIN.

use crate::error::{Error, Result};
use crate::url::Url;

/// LOGIN (using userinfo or fall back to anonymous), SELECT the mailbox from
/// `url.path`, then either LIST mailboxes or FETCH a specific message and
/// return the raw RFC 5322 message bytes (or the LIST output).
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme(
        "imap/imaps not yet implemented".into(),
    ))
}
