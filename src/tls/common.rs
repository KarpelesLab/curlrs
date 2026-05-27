//! Types shared by every TLS backend (currently `purecrypto` and `rustls`).
//!
//! Keeping a small backend-neutral surface here means consumer code never
//! has to name a crate-specific TLS type, so flipping the `rustls-tls`
//! feature switches the implementation transparently.

/// Negotiated TLS protocol version, mapped from whichever backend ran the
/// handshake. The `Debug` derive prints `TLSv1_3` / `TLSv1_2`, which is the
/// form the verbose trace in `src/http.rs` already shows via `{v:?}`.
///
/// `Other(u16)` is used for anything outside the two TLS 1.x versions we
/// currently advertise — its `u16` is the on-wire two-byte version code
/// (e.g. `0x0301` for TLS 1.0) so a diagnostic still has something to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[allow(non_camel_case_types)]
pub enum ProtocolVersion {
    TLSv1_2,
    TLSv1_3,
    Other(u16),
}
