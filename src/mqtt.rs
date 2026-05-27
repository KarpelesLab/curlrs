//! MQTT and MQTTS support.
//!
//! Spec: MQTT v3.1.1 (OASIS, also ISO/IEC 20922:2016) — curl supports v3.1.1.
//! v5 may be added later. URL format: `mqtt://host[:1883]/topic`.
//!
//! Use [`crate::tls::connect_over`] for `mqtts://`.

use crate::error::{Error, Result};
use crate::url::Url;

/// CONNECT, SUBSCRIBE to the topic in `url.path`, return the payload of the
/// first PUBLISH received, then DISCONNECT.
pub fn fetch(_url: &Url) -> Result<Vec<u8>> {
    Err(Error::UnsupportedScheme(
        "mqtt/mqtts not yet implemented".into(),
    ))
}
