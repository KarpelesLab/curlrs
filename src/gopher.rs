//! Gopher and Gopher-over-TLS support (RFC 1436 + the TLS extension).
//!
//! Gopher URLs are `gopher://host[:70]/<type><selector>` where `<type>` is a
//! single character item type (e.g. `1` directory, `0` text file). For TLS
//! use [`crate::tls::connect_over`].
//!
//! Gopher has no length framing: the server writes the response and then
//! closes the connection, so the client reads to EOF.

use std::io::{Read, Write};
use std::net::TcpStream;

use crate::error::Result;
use crate::url::Url;

/// Send the selector from `url.path` and read the server's response until
/// the connection is closed (gopher has no length framing).
pub fn fetch(url: &Url) -> Result<Vec<u8>> {
    let addr = format!("{}:{}", url.host, url.port);
    let tcp = TcpStream::connect(&addr)?;

    let selector = selector_from_path(&url.path);
    let mut request = Vec::with_capacity(selector.len() + 2);
    request.extend_from_slice(selector.as_bytes());
    request.extend_from_slice(b"\r\n");

    if url.is_tls() {
        let mut tls = crate::tls::connect_over(tcp, &url.host)?;
        tls.write_all(&request)?;
        tls.flush()?;
        let mut buf = Vec::new();
        tls.read_to_end(&mut buf)?;
        Ok(buf)
    } else {
        let mut sock = tcp;
        sock.write_all(&request)?;
        sock.flush()?;
        let mut buf = Vec::new();
        sock.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

/// Extract the wire selector from a Gopher URL path.
///
/// A Gopher URL path is `/<itemtype><selector>` where `<itemtype>` is a single
/// byte and the selector is everything after it. The item-type byte is *not*
/// part of the wire selector; it's only a hint to the client about how to
/// render the response.
///
/// * `""` or `"/"` → empty selector (root menu, defaults to type `1`).
/// * `"/1"` → empty selector (root menu, explicit directory type).
/// * `"/0foo"` → `"foo"` (text file selector).
/// * `"/1docs/index"` → `"docs/index"` (directory selector).
fn selector_from_path(path: &str) -> &str {
    // Strip leading slash if present.
    let without_slash = path.strip_prefix('/').unwrap_or(path);
    // Drop the item-type byte (first char), if any.
    let mut chars = without_slash.chars();
    match chars.next() {
        Some(_) => chars.as_str(),
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_root_slash() {
        assert_eq!(selector_from_path("/"), "");
    }

    #[test]
    fn selector_empty() {
        assert_eq!(selector_from_path(""), "");
    }

    #[test]
    fn selector_just_item_type() {
        assert_eq!(selector_from_path("/1"), "");
    }

    #[test]
    fn selector_text_file() {
        assert_eq!(selector_from_path("/0foo"), "foo");
    }

    #[test]
    fn selector_directory_with_subpath() {
        assert_eq!(selector_from_path("/1docs/index"), "docs/index");
    }
}
