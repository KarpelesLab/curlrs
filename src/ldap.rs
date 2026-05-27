//! LDAP and LDAPS support.
//!
//! Specs: RFC 4511 (LDAP protocol), RFC 4516 (LDAP URL format),
//! RFC 4513 (LDAP authentication / TLS).
//!
//! LDAP URLs look like `ldap://host/dn?attrs?scope?filter?extensions`.
//! Protocol messages are BER-encoded; [`purecrypto::der`] provides a DER
//! reader/writer that is the natural building block for BER too (LDAP uses
//! a constrained BER subset where length forms must be minimal —
//! effectively DER).

use crate::error::{Error, Result};
use crate::url::Url;

/// Bind (anonymous unless userinfo is set), run the search described by
/// `url.path` + query, and return the search results serialized as LDIF.
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme(
        "ldap/ldaps not yet implemented".into(),
    ))
}
