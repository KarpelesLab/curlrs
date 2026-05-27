//! POP3 and POP3S support.
//!
//! Specs: RFC 1939 (POP3), RFC 2595 (POP3 over TLS / STLS), RFC 8314
//! (implicit TLS on port 995 for POP3S), RFC 2384 (`pop3:` URL scheme).
//!
//! URLs: `pop3://user:pass@host/` (LIST), `pop3://user:pass@host/1` (RETR 1).
//! For POP3S, connect with TLS from the start via
//! [`crate::tls::connect_over`].

use crate::error::{Error, Result};
use crate::url::Url;

/// USER + PASS auth, then either LIST mailboxes (if no message number in
/// path) or RETR a specific message. Returns the raw bytes (RFC 5322 message
/// or the textual LIST output).
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme(
        "pop3/pop3s not yet implemented".into(),
    ))
}
