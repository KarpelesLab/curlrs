//! Universal "give me the bytes" entry point that dispatches on URL scheme.
//!
//! This is the API the CLI uses for non-HTTP URLs and the natural front door
//! for callers that just want curl-like behavior across many protocols.
//!
//! For HTTP/HTTPS, prefer [`crate::Request`] / [`crate::get`] directly —
//! they expose status, headers, and the full response model. `transfer`
//! discards everything except the body for those schemes.

use crate::error::{Error, Result};
use crate::url::Url;

/// Run the default operation for the URL's scheme and return its payload.
pub fn transfer(url_str: &str) -> Result<Vec<u8>> {
    let url = Url::parse(url_str)?;
    transfer_url(&url)
}

/// Same as [`transfer`] but starts from an already-parsed URL.
pub fn transfer_url(url: &Url) -> Result<Vec<u8>> {
    match url.scheme.as_str() {
        "http" | "https" => crate::Request::get(&format!(
            "{}://{}{}{}",
            url.scheme,
            url.host,
            if (url.scheme == "http" && url.port == 80)
                || (url.scheme == "https" && url.port == 443)
            {
                String::new()
            } else {
                format!(":{}", url.port)
            },
            url.path
        ))?
        .send()
        .map(|r| r.body),
        "ftp" | "ftps" => crate::ftp::fetch(url),
        "dict" => crate::dict::fetch(url),
        "file" => crate::file::fetch(url),
        "gopher" | "gophers" => crate::gopher::fetch(url),
        "imap" | "imaps" => crate::imap::fetch(url),
        "ldap" | "ldaps" => crate::ldap::fetch(url),
        "mqtt" | "mqtts" => crate::mqtt::fetch(url),
        "pop3" | "pop3s" => crate::pop3::fetch(url),
        "rtsp" => crate::rtsp::fetch(url),
        "tftp" => crate::tftp::fetch(url),
        "ws" | "wss" => crate::websocket::fetch(url),
        other => Err(Error::UnsupportedScheme(other.to_string())),
    }
}
