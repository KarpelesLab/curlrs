//! MQTT and MQTTS support.
//!
//! Spec: MQTT v3.1.1 (OASIS, also ISO/IEC 20922:2016) — curl supports v3.1.1.
//! v5 may be added later. URL format: `mqtt://host[:1883]/topic`.
//!
//! Use [`crate::tls::connect_over`] for `mqtts://`.
//!
//! The current flow is intentionally simple and matches what curl does for
//! `mqtt://`: CONNECT → SUBSCRIBE → wait for the first PUBLISH → DISCONNECT.
//! QoS is hard-coded to 0; QoS 1/2 (with PUBACK / PUBREC / PUBREL / PUBCOMP),
//! retained-message flags on outbound packets, last-will, and MQTT v5 are not
//! implemented.

use std::io::{self, Read, Write};
use std::net::TcpStream;

use purecrypto::rng::{OsRng, RngCore};

use crate::error::{Error, Result};
use crate::url::Url;

// MQTT v3.1.1 control packet types (high nibble of the fixed-header byte).
const PKT_CONNECT: u8 = 1;
const PKT_CONNACK: u8 = 2;
const PKT_PUBLISH: u8 = 3;
const PKT_SUBSCRIBE: u8 = 8;
const PKT_SUBACK: u8 = 9;
const PKT_PINGRESP: u8 = 13;
const PKT_DISCONNECT: u8 = 14;

/// CONNECT, SUBSCRIBE to the topic in `url.path`, return the payload of the
/// first PUBLISH received, then DISCONNECT.
pub fn fetch(url: &Url) -> Result<Vec<u8>> {
    let topic = url.path.strip_prefix('/').unwrap_or(&url.path);
    if topic.is_empty() {
        return Err(Error::InvalidUrl(format!(
            "mqtt: no topic in URL path ({:?})",
            url.path
        )));
    }

    let (user, pass) = split_userinfo(url.userinfo.as_deref());

    let addr = format!("{}:{}", url.host, url.port);
    let tcp = TcpStream::connect(&addr)?;
    if url.is_tls() {
        let mut stream = crate::tls::connect_over(tcp, &url.host)?;
        run_session(&mut stream, topic, user, pass)
    } else {
        let mut stream = tcp;
        run_session(&mut stream, topic, user, pass)
    }
}

fn run_session<S: Read + Write>(
    stream: &mut S,
    topic: &str,
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<Vec<u8>> {
    let client_id = random_client_id();
    let connect = build_connect(&client_id, user, pass, 60);
    stream.write_all(&connect)?;
    stream.flush()?;

    // CONNACK: type 2, remaining length 2, variable header is
    // [session-present flag, return code].
    let (ctype, body) = read_packet(stream)?;
    if ctype != PKT_CONNACK {
        return Err(Error::BadResponse(format!(
            "mqtt: expected CONNACK, got packet type {ctype}"
        )));
    }
    if body.len() < 2 {
        return Err(Error::BadResponse("mqtt: short CONNACK".into()));
    }
    let rc = body[1];
    if rc != 0 {
        return Err(Error::BadResponse(format!("mqtt: connack {rc}")));
    }

    // SUBSCRIBE with packet id 1, single topic at QoS 0.
    let subscribe = build_subscribe(1, topic);
    stream.write_all(&subscribe)?;
    stream.flush()?;

    // SUBACK: type 9, payload is [packet_id_msb, packet_id_lsb, return_code...].
    let (ctype, body) = read_packet(stream)?;
    if ctype != PKT_SUBACK {
        return Err(Error::BadResponse(format!(
            "mqtt: expected SUBACK, got packet type {ctype}"
        )));
    }
    if body.len() < 3 {
        return Err(Error::BadResponse("mqtt: short SUBACK".into()));
    }
    let sub_rc = body[2];
    if sub_rc == 0x80 {
        return Err(Error::BadResponse("mqtt: suback failure (0x80)".into()));
    }

    // Drain packets until we get a PUBLISH. We just ignore anything else
    // (e.g. PINGRESP if the server pings us first), which is enough for the
    // simple "subscribe and get one message" flow.
    let payload = loop {
        let (ctype, body) = read_packet(stream)?;
        match ctype {
            PKT_PUBLISH => break extract_publish_payload(&body)?,
            PKT_PINGRESP => continue,
            other => {
                return Err(Error::BadResponse(format!(
                    "mqtt: unexpected packet type {other} before PUBLISH"
                )));
            }
        }
    };

    // DISCONNECT is a 2-byte fixed header with no remaining length payload.
    let _ = stream.write_all(&[PKT_DISCONNECT << 4, 0x00]);
    let _ = stream.flush();

    Ok(payload)
}

/// Split a `user[:pass]` userinfo string. Both halves are returned as
/// borrowed slices into the original string.
fn split_userinfo(ui: Option<&str>) -> (Option<&str>, Option<&str>) {
    match ui {
        None => (None, None),
        Some(s) => match s.split_once(':') {
            Some((u, p)) => (Some(u), Some(p)),
            None => (Some(s), None),
        },
    }
}

/// Generate a fresh `curlrs-XXXXXXXXXXXX` client id (12 lowercase hex chars,
/// 48 bits of randomness from the OS CSPRNG).
fn random_client_id() -> String {
    let mut buf = [0u8; 6];
    OsRng.fill_bytes(&mut buf);
    let mut s = String::with_capacity(7 + 12);
    s.push_str("curlrs-");
    for b in buf {
        s.push(hex_nibble(b >> 4));
        s.push(hex_nibble(b & 0x0F));
    }
    s
}

fn hex_nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + n - 10) as char,
        _ => unreachable!(),
    }
}

/// Append `s` to `out` prefixed by its UTF-8 byte length as a big-endian u16.
/// MQTT v3.1.1 caps any single string at 65535 bytes; longer strings are
/// truncated here defensively (callers control these values).
fn push_str(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(u16::MAX as usize) as u16;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&bytes[..len as usize]);
}

/// Build the full bytes of a CONNECT packet (fixed header + variable header +
/// payload), with the "clean session" flag always set.
pub(crate) fn build_connect(
    client_id: &str,
    user: Option<&str>,
    pass: Option<&str>,
    keep_alive_secs: u16,
) -> Vec<u8> {
    // Variable header.
    let mut vh = Vec::new();
    push_str(&mut vh, "MQTT");
    vh.push(4); // Protocol level: MQTT v3.1.1
    let mut flags: u8 = 0x02; // clean session
    if user.is_some() {
        flags |= 0x80;
    }
    if pass.is_some() {
        flags |= 0x40;
    }
    vh.push(flags);
    vh.extend_from_slice(&keep_alive_secs.to_be_bytes());

    // Payload.
    let mut pl = Vec::new();
    push_str(&mut pl, client_id);
    if let Some(u) = user {
        push_str(&mut pl, u);
    }
    if let Some(p) = pass {
        push_str(&mut pl, p);
    }

    let mut out = Vec::with_capacity(2 + vh.len() + pl.len());
    out.push(PKT_CONNECT << 4); // 0x10
    write_remaining_length(&mut out, vh.len() + pl.len());
    out.extend_from_slice(&vh);
    out.extend_from_slice(&pl);
    out
}

/// Build a SUBSCRIBE for a single `topic` at QoS 0.
pub(crate) fn build_subscribe(packet_id: u16, topic: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&packet_id.to_be_bytes());
    push_str(&mut body, topic);
    body.push(0x00); // QoS 0

    let mut out = Vec::with_capacity(2 + body.len());
    // SUBSCRIBE requires the lower nibble to be 0b0010 per MQTT v3.1.1 §3.8.1.
    out.push((PKT_SUBSCRIBE << 4) | 0x02); // 0x82
    write_remaining_length(&mut out, body.len());
    out.extend_from_slice(&body);
    out
}

/// Extract the application payload from a PUBLISH packet body.
///
/// We only handle QoS 0 here, which is all `build_subscribe` ever requests:
/// the variable header is just `<topic-name>` and the rest is the payload.
fn extract_publish_payload(body: &[u8]) -> Result<Vec<u8>> {
    if body.len() < 2 {
        return Err(Error::BadResponse("mqtt: short PUBLISH".into()));
    }
    let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
    let after_topic = 2 + topic_len;
    if body.len() < after_topic {
        return Err(Error::BadResponse(
            "mqtt: PUBLISH topic length exceeds packet".into(),
        ));
    }
    Ok(body[after_topic..].to_vec())
}

/// Read a single MQTT packet: one fixed-header byte, a variable-length
/// "remaining length", then exactly that many bytes of body.
/// Returns `(packet_type_nibble, body_bytes)`.
fn read_packet<R: Read>(r: &mut R) -> Result<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 1];
    read_exact_or_eof(r, &mut hdr)?;
    let ctype = hdr[0] >> 4;
    let rem = read_remaining_length(r)?;
    let mut body = vec![0u8; rem];
    if rem > 0 {
        read_exact_or_eof(r, &mut body)?;
    }
    Ok((ctype, body))
}

fn read_exact_or_eof<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<()> {
    match r.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Err(Error::UnexpectedEof),
        Err(e) => Err(Error::Io(e)),
    }
}

/// Decode a MQTT "remaining length" varint: 1-4 bytes, low 7 bits are value,
/// high bit is the continuation flag. Returns the parsed length in bytes.
///
/// Per the spec the maximum legal value is 268_435_455 (0xFD,0xFF,0xFF,0x7F).
/// A 5th byte (or any 4th byte with the continuation bit set) is malformed.
pub(crate) fn read_remaining_length<R: Read>(r: &mut R) -> io::Result<usize> {
    let mut value: usize = 0;
    let mut multiplier: usize = 1;
    for i in 0..4 {
        let mut b = [0u8; 1];
        r.read_exact(&mut b)?;
        let byte = b[0];
        value += (byte & 0x7F) as usize * multiplier;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        // Last legal byte must not have the continuation bit set.
        if i == 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "mqtt: malformed remaining length (5th byte)",
            ));
        }
        multiplier *= 128;
    }
    unreachable!("loop returns or errors before exit")
}

/// Encode `len` as a MQTT "remaining length" varint and append it to `out`.
///
/// `len` must fit in 28 bits (`<= 268_435_455`); larger values are clamped at
/// the maximum since the caller controls the packets we produce.
pub(crate) fn write_remaining_length(out: &mut Vec<u8>, len: usize) {
    let mut x = len.min(268_435_455);
    loop {
        let mut byte = (x & 0x7F) as u8;
        x >>= 7;
        if x > 0 {
            byte |= 0x80;
            out.push(byte);
        } else {
            out.push(byte);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spec boundary values: each is the max of one varint length, and the
    // first value that needs one more byte.
    const RL_CASES: &[(usize, &[u8])] = &[
        (0, &[0x00]),
        (127, &[0x7F]),
        (128, &[0x80, 0x01]),
        (16_383, &[0xFF, 0x7F]),
        (16_384, &[0x80, 0x80, 0x01]),
        (2_097_151, &[0xFF, 0xFF, 0x7F]),
        (2_097_152, &[0x80, 0x80, 0x80, 0x01]),
        (268_435_455, &[0xFF, 0xFF, 0xFF, 0x7F]),
    ];

    #[test]
    fn write_remaining_length_matches_spec_bytes() {
        for (value, expected) in RL_CASES {
            let mut buf = Vec::new();
            write_remaining_length(&mut buf, *value);
            assert_eq!(
                buf.as_slice(),
                *expected,
                "encoding of {value} (got {:02X?}, want {:02X?})",
                buf,
                expected
            );
        }
    }

    #[test]
    fn read_remaining_length_round_trips() {
        for (value, expected) in RL_CASES {
            // Round-trip: write then read.
            let mut buf = Vec::new();
            write_remaining_length(&mut buf, *value);
            let mut cur = std::io::Cursor::new(&buf);
            let got = read_remaining_length(&mut cur).expect("decode");
            assert_eq!(got, *value, "round trip for {value}");
            // Also: the canonical spec encoding decodes to the same number.
            let mut cur2 = std::io::Cursor::new(*expected);
            let got2 = read_remaining_length(&mut cur2).expect("decode spec bytes");
            assert_eq!(got2, *value, "spec-bytes decode for {value}");
        }
    }

    #[test]
    fn read_remaining_length_rejects_5_byte_varint() {
        // Four continuation bytes is illegal — the 4th byte must be terminal.
        let bad = [0xFF, 0xFF, 0xFF, 0xFF];
        let mut cur = std::io::Cursor::new(&bad[..]);
        assert!(read_remaining_length(&mut cur).is_err());
    }

    #[test]
    fn build_connect_exact_bytes_for_known_input() {
        // CONNECT with client_id "abc", no user/pass, keep-alive 60.
        //
        // Variable header (10 bytes):
        //   00 04 'M' 'Q' 'T' 'T'   -- protocol name
        //   04                       -- protocol level (v3.1.1)
        //   02                       -- connect flags: clean session
        //   00 3C                    -- keep alive = 60
        // Payload (5 bytes):
        //   00 03 'a' 'b' 'c'        -- client id
        // Remaining length = 15 = 0x0F
        // Fixed header: 0x10 0x0F
        let got = build_connect("abc", None, None, 60);
        let expected: Vec<u8> = vec![
            0x10, 0x0F, // fixed header: CONNECT, remaining length 15
            0x00, 0x04, b'M', b'Q', b'T', b'T', // protocol name
            0x04, // protocol level
            0x02, // flags
            0x00, 0x3C, // keep alive
            0x00, 0x03, b'a', b'b', b'c', // client id
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn build_connect_sets_user_and_password_flags() {
        let got = build_connect("id", Some("u"), Some("p"), 30);
        // Variable header (10):
        //   00 04 M Q T T  04  C2  00 1E
        //     flags = 0x02 | 0x80 (user) | 0x40 (pass) = 0xC2
        // Payload (4 + 3 + 3 = 10):
        //   00 02 'i' 'd'   00 01 'u'   00 01 'p'
        // Remaining length = 20 = 0x14
        let expected: Vec<u8> = vec![
            0x10, 0x14, 0x00, 0x04, b'M', b'Q', b'T', b'T', 0x04, 0xC2, 0x00, 0x1E, 0x00, 0x02,
            b'i', b'd', 0x00, 0x01, b'u', 0x00, 0x01, b'p',
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn build_subscribe_exact_bytes() {
        // SUBSCRIBE packet id 1, topic "a/b", QoS 0.
        //   fixed: 0x82, rem-length
        //   body: 00 01 (packet id), 00 03 'a' '/' 'b' (topic), 00 (QoS 0)
        //   body length = 2 + 5 + 1 = 8
        let got = build_subscribe(1, "a/b");
        let expected: Vec<u8> = vec![0x82, 0x08, 0x00, 0x01, 0x00, 0x03, b'a', b'/', b'b', 0x00];
        assert_eq!(got, expected);
    }

    #[test]
    fn extract_publish_payload_strips_topic() {
        // body: 00 03 't' 'o' 'p'  P A Y
        let body = b"\x00\x03topPAY";
        let payload = extract_publish_payload(body).unwrap();
        assert_eq!(payload, b"PAY");
    }

    #[test]
    fn split_userinfo_variants() {
        assert_eq!(split_userinfo(None), (None, None));
        assert_eq!(split_userinfo(Some("alice")), (Some("alice"), None));
        assert_eq!(
            split_userinfo(Some("alice:secret")),
            (Some("alice"), Some("secret"))
        );
        // The first ':' is the split point — passwords may contain colons,
        // which is intentional and matches what curl does.
        assert_eq!(
            split_userinfo(Some("alice:s:p")),
            (Some("alice"), Some("s:p"))
        );
    }

    #[test]
    fn random_client_id_format() {
        let id = random_client_id();
        assert!(id.starts_with("curlrs-"), "got {id}");
        let suffix = &id["curlrs-".len()..];
        assert_eq!(suffix.len(), 12);
        assert!(suffix
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // Two calls should not collide (48 bits of entropy).
        assert_ne!(random_client_id(), random_client_id());
    }
}
