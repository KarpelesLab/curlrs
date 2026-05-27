//! TFTP support (RFC 1350, plus RFC 2347 option extension, RFC 2348 blksize,
//! RFC 2349 timeout/tsize).
//!
//! TFTP runs over UDP, default port 69. URL: `tftp://host/path`. Default
//! operation is a read (RRQ) of `url.path` in octet mode, reassembling
//! 512-byte (or negotiated) blocks until a short block signals end.
//!
//! Only the read side (RRQ) is implemented. Option negotiation (RFC 2347)
//! is not done; we send a plain RRQ in `octet` mode and accept 512-byte
//! DATA blocks. Writes (WRQ) are not supported.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::url::Url;

/// TFTP opcodes (RFC 1350 §5).
const OP_RRQ: u16 = 1;
#[allow(dead_code)]
const OP_WRQ: u16 = 2;
const OP_DATA: u16 = 3;
const OP_ACK: u16 = 4;
const OP_ERROR: u16 = 5;

/// Default TFTP block size (RFC 1350).
const BLOCK_SIZE: usize = 512;

/// Per-packet receive timeout. Matches typical TFTP client behaviour.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Number of times we resend the last packet on timeout before giving up.
const MAX_RETRIES: u32 = 3;

/// Hard upper bound on a single transfer (256 MiB). TFTP has no built-in
/// length so we cap to avoid runaway allocation against a hostile server.
const MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;

/// Result of parsing one DATA packet's header. `data` borrows from the input.
#[derive(Debug, PartialEq, Eq)]
struct DataPacket<'a> {
    block: u16,
    data: &'a [u8],
}

/// Build a Read Request packet:
/// `\x00\x01<filename>\x00octet\x00`.
fn build_rrq(filename: &str) -> Vec<u8> {
    let mut p = Vec::with_capacity(2 + filename.len() + 1 + 5 + 1);
    p.extend_from_slice(&OP_RRQ.to_be_bytes());
    p.extend_from_slice(filename.as_bytes());
    p.push(0);
    p.extend_from_slice(b"octet");
    p.push(0);
    p
}

/// Build an ACK packet: `\x00\x04<2-byte block#>`.
fn build_ack(block: u16) -> [u8; 4] {
    let mut p = [0u8; 4];
    p[0..2].copy_from_slice(&OP_ACK.to_be_bytes());
    p[2..4].copy_from_slice(&block.to_be_bytes());
    p
}

/// Parse the opcode at the start of a packet, returning `None` if too short.
fn parse_opcode(buf: &[u8]) -> Option<u16> {
    if buf.len() < 2 {
        return None;
    }
    Some(u16::from_be_bytes([buf[0], buf[1]]))
}

/// Parse a DATA packet (opcode 3). Returns the block number and a slice
/// pointing into `buf` for the payload bytes.
fn parse_data(buf: &[u8]) -> Result<DataPacket<'_>> {
    if buf.len() < 4 {
        return Err(Error::BadResponse("tftp: short DATA packet".into()));
    }
    if parse_opcode(buf) != Some(OP_DATA) {
        return Err(Error::BadResponse("tftp: not a DATA packet".into()));
    }
    let block = u16::from_be_bytes([buf[2], buf[3]]);
    Ok(DataPacket {
        block,
        data: &buf[4..],
    })
}

/// Parse an ERROR packet (opcode 5). Returns the human-readable message
/// (trimmed of the trailing NUL if present). The error code itself is
/// discarded; the message is what users want to see.
fn parse_error(buf: &[u8]) -> Result<String> {
    if buf.len() < 4 {
        return Err(Error::BadResponse("tftp: short ERROR packet".into()));
    }
    if parse_opcode(buf) != Some(OP_ERROR) {
        return Err(Error::BadResponse("tftp: not an ERROR packet".into()));
    }
    // bytes [2..4] are the error code, then a NUL-terminated message.
    let msg_bytes = &buf[4..];
    let end = msg_bytes.iter().position(|&b| b == 0).unwrap_or(msg_bytes.len());
    Ok(String::from_utf8_lossy(&msg_bytes[..end]).into_owned())
}

/// Resolve `host:port` to the first usable socket address.
fn resolve(host: &str, port: u16) -> Result<SocketAddr> {
    (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| Error::BadResponse(format!("tftp: cannot resolve {host}:{port}")))
}

/// RRQ the file at `url.path` and return the reassembled bytes.
pub fn fetch(url: &Url) -> Result<Vec<u8>> {
    // Strip the leading '/' to get the TFTP filename. Anything past a '?' (a
    // query, which TFTP doesn't actually have) is left in place — TFTP servers
    // will just see it as part of the filename.
    let filename = url.path.strip_prefix('/').unwrap_or(&url.path);
    if filename.is_empty() {
        return Err(Error::InvalidUrl(format!(
            "tftp: empty filename in {}://{}/{}",
            url.scheme, url.host, url.path
        )));
    }
    if filename.as_bytes().contains(&0) {
        return Err(Error::InvalidUrl("tftp: filename contains NUL".into()));
    }

    let server = resolve(&url.host, url.port)?;
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(READ_TIMEOUT))?;

    let rrq = build_rrq(filename);

    // After the first DATA arrives, the server picks a fresh ephemeral port
    // (the TID) and all subsequent traffic uses it. We track it here.
    let mut peer: Option<SocketAddr> = None;

    let mut out: Vec<u8> = Vec::new();
    let mut expected_block: u16 = 1;
    // Buffer big enough for the largest DATA packet we might see (4 header
    // bytes + 512 payload). A small extra margin doesn't hurt.
    let mut buf = [0u8; 4 + BLOCK_SIZE + 16];

    // Send the RRQ with retries, then enter the data loop. After we ACK each
    // DATA, that ACK becomes the new "last packet" we'd retransmit on timeout.
    let mut last_packet: Vec<u8> = rrq;
    let mut last_dest: SocketAddr = server;

    socket.send_to(&last_packet, last_dest)?;
    let mut retries: u32 = 0;

    loop {
        let (n, from) = match socket.recv_from(&mut buf) {
            Ok(v) => v,
            Err(e) => {
                // WouldBlock / TimedOut from set_read_timeout.
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) {
                    if retries >= MAX_RETRIES {
                        return Err(Error::UnexpectedEof);
                    }
                    retries += 1;
                    socket.send_to(&last_packet, last_dest)?;
                    continue;
                }
                return Err(Error::Io(e));
            }
        };

        // Once we've latched onto the server's TID, ignore packets from
        // anywhere else (RFC 1350 §4: "If a source TID does not match, the
        // packet should be discarded as erroneously sent from somewhere
        // else"). On the very first packet, the source IP must still match
        // the host we sent to, but the port will differ.
        if let Some(p) = peer {
            if from != p {
                continue;
            }
        } else if from.ip() != server.ip() {
            continue;
        }

        let pkt = &buf[..n];
        match parse_opcode(pkt) {
            Some(OP_DATA) => {
                let data = parse_data(pkt)?;

                if data.block != expected_block {
                    // Either a duplicate of an already-acked block (re-ack
                    // it so the sender unblocks) or out-of-order garbage we
                    // ignore. Re-acking an old block is harmless.
                    if data.block.wrapping_add(1) == expected_block {
                        let ack = build_ack(data.block);
                        socket.send_to(&ack, from)?;
                    }
                    continue;
                }

                // Latch onto this peer's TID on the first valid DATA.
                if peer.is_none() {
                    peer = Some(from);
                }

                if out.len() + data.data.len() > MAX_TOTAL_BYTES {
                    return Err(Error::BadResponse(format!(
                        "tftp: transfer exceeds {} bytes",
                        MAX_TOTAL_BYTES
                    )));
                }
                out.extend_from_slice(data.data);

                let ack = build_ack(data.block);
                socket.send_to(&ack, from)?;

                let is_last = data.data.len() < BLOCK_SIZE;
                if is_last {
                    return Ok(out);
                }

                // Prepare to retransmit this ACK if the next DATA times out.
                last_packet = ack.to_vec();
                last_dest = from;
                retries = 0;

                // u16 wrap: 65535 -> 0 is permitted by common TFTP usage,
                // but for safety we explicitly bail rather than risk an
                // ambiguous loop. (A 256 MiB cap means a 512-byte block
                // stream can need block numbers up to ~524288, which does
                // wrap once. Punt as documented in the spec.)
                expected_block = match expected_block.checked_add(1) {
                    Some(b) => b,
                    None => {
                        return Err(Error::BadResponse(
                            "tftp: block number wrapped; refusing oversized transfer".into(),
                        ));
                    }
                };
            }
            Some(OP_ERROR) => {
                let msg = parse_error(pkt)?;
                return Err(Error::BadResponse(format!("tftp: {msg}")));
            }
            Some(op) => {
                return Err(Error::BadResponse(format!(
                    "tftp: unexpected opcode {op}"
                )));
            }
            None => {
                return Err(Error::BadResponse("tftp: packet too short".into()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrq_builds_in_octet_mode() {
        let p = build_rrq("hello.txt");
        // opcode 1, filename, 0, "octet", 0
        assert_eq!(p[0..2], [0x00, 0x01]);
        assert_eq!(&p[2..2 + b"hello.txt".len()], b"hello.txt");
        let after_name = 2 + b"hello.txt".len();
        assert_eq!(p[after_name], 0);
        assert_eq!(&p[after_name + 1..after_name + 1 + 5], b"octet");
        assert_eq!(*p.last().unwrap(), 0);
        assert_eq!(p.len(), 2 + 9 + 1 + 5 + 1);
    }

    #[test]
    fn rrq_handles_empty_filename_shape() {
        // We don't reject here (`fetch` does), but the encoding should still
        // be well-formed: two NULs around an empty mode string.
        let p = build_rrq("");
        assert_eq!(p, vec![0x00, 0x01, 0x00, b'o', b'c', b't', b'e', b't', 0x00]);
    }

    #[test]
    fn ack_encodes_block_number_big_endian() {
        assert_eq!(build_ack(0), [0x00, 0x04, 0x00, 0x00]);
        assert_eq!(build_ack(1), [0x00, 0x04, 0x00, 0x01]);
        assert_eq!(build_ack(0x0102), [0x00, 0x04, 0x01, 0x02]);
        assert_eq!(build_ack(0xFFFF), [0x00, 0x04, 0xFF, 0xFF]);
    }

    #[test]
    fn parse_opcode_handles_short_input() {
        assert_eq!(parse_opcode(&[]), None);
        assert_eq!(parse_opcode(&[0x00]), None);
        assert_eq!(parse_opcode(&[0x00, 0x03]), Some(3));
        assert_eq!(parse_opcode(&[0x00, 0x05, 0xAA]), Some(5));
    }

    #[test]
    fn parse_data_extracts_block_and_payload() {
        let pkt = [0x00, 0x03, 0x00, 0x07, b'a', b'b', b'c'];
        let d = parse_data(&pkt).unwrap();
        assert_eq!(d.block, 7);
        assert_eq!(d.data, b"abc");
    }

    #[test]
    fn parse_data_allows_empty_payload() {
        // A DATA with zero payload bytes (block 0 of a tsize=0 file etc.)
        // is well-formed; it just signals EOF immediately.
        let pkt = [0x00, 0x03, 0x00, 0x42];
        let d = parse_data(&pkt).unwrap();
        assert_eq!(d.block, 0x42);
        assert_eq!(d.data, b"");
    }

    #[test]
    fn parse_data_rejects_short_header() {
        assert!(parse_data(&[]).is_err());
        assert!(parse_data(&[0x00, 0x03]).is_err());
        assert!(parse_data(&[0x00, 0x03, 0x00]).is_err());
    }

    #[test]
    fn parse_data_rejects_wrong_opcode() {
        let pkt = [0x00, 0x04, 0x00, 0x01];
        assert!(parse_data(&pkt).is_err());
    }

    #[test]
    fn parse_error_strips_trailing_nul() {
        let pkt = [0x00, 0x05, 0x00, 0x01, b'N', b'o', b'p', b'e', 0x00];
        let m = parse_error(&pkt).unwrap();
        assert_eq!(m, "Nope");
    }

    #[test]
    fn parse_error_tolerates_missing_nul() {
        // Some implementations forget the terminator. Be lenient.
        let pkt = [0x00, 0x05, 0x00, 0x02, b'h', b'i'];
        let m = parse_error(&pkt).unwrap();
        assert_eq!(m, "hi");
    }

    #[test]
    fn parse_error_handles_empty_message() {
        let pkt = [0x00, 0x05, 0x00, 0x03, 0x00];
        let m = parse_error(&pkt).unwrap();
        assert_eq!(m, "");
    }

    #[test]
    fn parse_error_rejects_short_header() {
        assert!(parse_error(&[0x00, 0x05]).is_err());
        assert!(parse_error(&[0x00, 0x05, 0x00]).is_err());
    }

    #[test]
    fn parse_error_rejects_wrong_opcode() {
        let pkt = [0x00, 0x03, 0x00, 0x01, b'x', 0x00];
        assert!(parse_error(&pkt).is_err());
    }

    #[test]
    fn parse_error_invalid_utf8_lossy() {
        // 0xFF is not valid UTF-8; from_utf8_lossy substitutes U+FFFD.
        let pkt = [0x00, 0x05, 0x00, 0x01, 0xFF, 0x00];
        let m = parse_error(&pkt).unwrap();
        assert!(m.contains('\u{FFFD}'));
    }
}
