//! WebSocket support (RFC 6455).
//!
//! WS handshakes are HTTP/1.1 `Upgrade: websocket` requests followed by
//! binary/text frames. Reuse the HTTP/1.1 request writer in
//! [`crate::http`] to perform the handshake (`Sec-WebSocket-Key`, etc.) and
//! then take over the raw stream for framing. For `wss://`, wrap the TCP
//! stream with [`crate::tls::connect_over`] before sending the upgrade.

use crate::error::{Error, Result};
use crate::url::Url;

/// Open a WS connection, read one text or binary data frame, close cleanly,
/// and return that frame's payload.
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme(
        "ws/wss not yet implemented".into(),
    ))
}
