//! Purecrypto root-store loading, available regardless of the active TLS
//! backend.
//!
//! HTTP/3 (`src/http3.rs`) is built on `purecrypto::quic`, which in turn is
//! built on `purecrypto::tls` — so HTTP/3 always needs a
//! `purecrypto::tls::RootCertStore`, even when the `rustls-tls` feature has
//! redirected [`crate::tls::load_system_roots`] to rustls. This module keeps
//! the purecrypto-flavoured loader unconditionally compiled so HTTP/3 has a
//! source of trust anchors regardless of which TLS backend is active.
//!
//! The `purecrypto-tls` backend just re-exports the two `load_*` functions
//! here as its public API surface; nothing else uses them.

use std::io;

use purecrypto::tls::RootCertStore;

use crate::error::{Error, Result};

/// Search paths for a system-wide CA bundle, in order of preference.
/// Mirrors what curl/OpenSSL look at on common Unix distros.
pub(crate) const SYSTEM_CA_PATHS: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt", // Debian/Ubuntu/Gentoo
    "/etc/pki/tls/certs/ca-bundle.crt",   // Fedora/RHEL
    "/etc/ssl/cert.pem",                  // Alpine, OpenBSD, macOS (via brew)
    "/etc/ssl/ca-bundle.pem",             // openSUSE
    "/etc/ca-certificates/extracted/tls-ca-bundle.pem", // Arch
];

/// Load every CA found in the first existing bundle on disk into a
/// `purecrypto::tls::RootCertStore`. PEM blocks that purecrypto cannot
/// parse (e.g. unsupported key types) are silently skipped, matching what
/// other pure-Rust TLS stacks do.
pub(crate) fn load_system_roots() -> Result<RootCertStore> {
    for path in SYSTEM_CA_PATHS {
        let pem = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(Error::Io(e)),
        };
        return parse_into_store(&pem, path);
    }
    Err(Error::BadResponse(
        "no system CA bundle found; tried common Unix paths".into(),
    ))
}

/// Load CA certificates from a user-supplied PEM bundle (curl's
/// `--cacert <file>`). Same parser as [`load_system_roots`]; empty bundle
/// is an error.
///
/// Only the purecrypto TLS backend wires this in today (the rustls backend
/// has its own loader); kept always-compiled and `allow(dead_code)` so any
/// future HTTP/3-side `--cacert` plumbing can use it without surgery.
#[allow(dead_code)]
pub(crate) fn load_from_file(path: &str) -> Result<RootCertStore> {
    let pem = std::fs::read_to_string(path).map_err(Error::Io)?;
    parse_into_store(&pem, path)
}

fn parse_into_store(pem: &str, path: &str) -> Result<RootCertStore> {
    let mut roots = RootCertStore::new();
    let mut loaded = 0usize;
    for block in pem_blocks(pem) {
        if roots.add_pem(&block).is_ok() {
            loaded += 1;
        }
    }
    if loaded == 0 {
        return Err(Error::BadResponse(format!(
            "no usable CA certificates parsed from {path}"
        )));
    }
    Ok(roots)
}

/// Yield each `-----BEGIN CERTIFICATE-----...-----END CERTIFICATE-----`
/// block from a PEM string as its own string.
pub(crate) fn pem_blocks(pem: &str) -> Vec<String> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let mut out = Vec::new();
    let mut rest = pem;
    while let Some(start) = rest.find(BEGIN) {
        let after_begin = &rest[start..];
        let Some(end_rel) = after_begin.find(END) else {
            break;
        };
        let end_abs = start + end_rel + END.len();
        out.push(rest[start..end_abs].to_string());
        rest = &rest[end_abs..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_blocks_splits() {
        let pem = "junk\n\
            -----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n\
            noise\n\
            -----BEGIN CERTIFICATE-----\nBBB\n-----END CERTIFICATE-----\n";
        let blocks = pem_blocks(pem);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("AAA"));
        assert!(blocks[1].contains("BBB"));
    }
}
