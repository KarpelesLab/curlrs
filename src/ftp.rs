//! FTP and FTPS support.
//!
//! Specs: RFC 959 (FTP), RFC 4217 (FTP over TLS / "explicit FTPS"),
//! plus the implicit-FTPS convention of TLS-from-start on port 990.
//!
//! This module implements the common-case read path:
//!   * Plain FTP (`ftp://`) and implicit FTPS (`ftps://`, TLS-from-connect).
//!   * Anonymous or `user:pass@` login.
//!   * Binary mode (`TYPE I`).
//!   * Passive data transfer: `EPSV`, with `PASV` fallback.
//!   * `RETR` for files, `LIST` for paths ending in `/`.
//!
//! Explicit `AUTH TLS` upgrade, active mode (`PORT`/`EPRT`), uploads
//! (`STOR`), and resume (`REST`) are intentionally not implemented yet —
//! `fetch` is purely a read API.
//!
//! For TLS we use [`crate::tls::connect_over`] on both the control channel
//! (on connect, for implicit FTPS) and the data channel (using the host
//! from the original URL as SNI, per RFC 4217 §10.2 — the passive reply
//! often carries an IP literal that wouldn't match the server cert).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

use crate::error::{Error, Result};
use crate::tls::TlsStream;
use crate::url::Url;

/// A duplex byte stream that's either a plain TCP socket or a TLS-wrapped
/// TCP socket. Lets us drive the same FTP state machine over both schemes
/// without trait objects (which would conflict with `TlsStream`'s generic
/// parameter and `BufReader`'s wrapping).
enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.read(buf),
            Stream::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.write(buf),
            Stream::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Plain(s) => s.flush(),
            Stream::Tls(s) => s.flush(),
        }
    }
}

/// Default operation: download the file at `url.path`, or list the directory
/// if the path ends in `/`. Returns the raw bytes.
pub fn fetch(url: &Url) -> Result<Vec<u8>> {
    if url.scheme != "ftp" && url.scheme != "ftps" {
        return Err(Error::UnsupportedScheme(url.scheme.clone()));
    }

    // 1) Control channel.
    let tcp = TcpStream::connect((url.host.as_str(), url.port))?;
    let control = if url.scheme == "ftps" {
        Stream::Tls(Box::new(crate::tls::connect_over(tcp, &url.host)?))
    } else {
        Stream::Plain(tcp)
    };
    let mut ctrl = BufReader::new(control);

    // 2) Banner (220 Service ready). Anything other than 1xx/2xx is fatal.
    let (code, _) = read_reply(&mut ctrl)?;
    if !is_positive(code) {
        return Err(Error::BadResponse(format!("ftp banner: {code}")));
    }

    // 3) Login. Anonymous by default; honor `user[:pass]@` from the URL.
    let (user, pass) = split_userinfo(url.userinfo.as_deref());
    send(&mut ctrl, &format!("USER {user}"))?;
    let (c, _) = read_reply(&mut ctrl)?;
    match c {
        230 => {} // logged in, no password needed
        331 => {
            // password required
            send(&mut ctrl, &format!("PASS {pass}"))?;
            let (c2, m2) = read_reply(&mut ctrl)?;
            if c2 != 230 && c2 != 202 {
                return Err(Error::BadResponse(format!("ftp PASS: {c2} {m2}")));
            }
        }
        332 => {
            return Err(Error::BadResponse(
                "ftp server requires ACCT, not supported".into(),
            ));
        }
        _ => return Err(Error::BadResponse(format!("ftp USER: {c}"))),
    }

    // 4) Binary mode.
    send(&mut ctrl, "TYPE I")?;
    let (c, m) = read_reply(&mut ctrl)?;
    if c != 200 {
        return Err(Error::BadResponse(format!("ftp TYPE I: {c} {m}")));
    }

    // 5) Passive: try EPSV first (works on v4 *and* v6, no parsing of host
    //    bytes needed), fall back to PASV on permanent failure.
    let (data_host, data_port) = open_passive(&mut ctrl, &url.host, url.port)?;

    // 6) Issue the transfer command BEFORE opening the data socket on some
    //    servers, AFTER on others; the spec allows either. We connect first
    //    (simpler) then send RETR/LIST. The server may answer 125/150
    //    before opening the data connection on its side, which is fine —
    //    we already have ours dialed.
    let data_tcp = TcpStream::connect((data_host.as_str(), data_port))?;
    let mut data = if url.scheme == "ftps" {
        // Per RFC 4217 §10.2: SNI must be the original hostname, not the
        // address we got from PASV/EPSV (which is often an IP literal).
        Stream::Tls(Box::new(crate::tls::connect_over(data_tcp, &url.host)?))
    } else {
        Stream::Plain(data_tcp)
    };

    // 7) RETR for files, LIST for directories. We treat a trailing '/' or
    //    the bare root path as "list this directory".
    let cmd = if url.path.is_empty() || url.path == "/" {
        "LIST".to_string()
    } else if url.path.ends_with('/') {
        format!("LIST {}", url.path)
    } else {
        format!("RETR {}", url.path)
    };
    send(&mut ctrl, &cmd)?;

    // 8) Preliminary reply (125 Data connection open / 150 File status OK).
    let (c, m) = read_reply(&mut ctrl)?;
    if !(c == 125 || c == 150) {
        // Some servers send the 226 directly (rare but legal). If we got an
        // error code, surface it.
        if !is_positive(c) {
            return Err(Error::BadResponse(format!("ftp {cmd}: {c} {m}")));
        }
    }

    // 9) Drain the data channel to EOF / TLS close_notify.
    let mut bytes = Vec::new();
    data.read_to_end(&mut bytes)?;
    // Dropping `data` closes both the TLS layer and the TCP socket.
    drop(data);

    // 10) Final reply (226 Closing data connection / Transfer complete).
    //     If we already saw the 226 above as the "preliminary" reply, no
    //     second one is coming — but we wouldn't have entered this branch
    //     because c would have been 226 (positive completion, not 125/150).
    if c == 125 || c == 150 {
        let (cf, mf) = read_reply(&mut ctrl)?;
        if !is_positive(cf) {
            return Err(Error::BadResponse(format!("ftp transfer end: {cf} {mf}")));
        }
    }

    // 11) Polite shutdown.
    let _ = send(&mut ctrl, "QUIT");
    let _ = read_reply(&mut ctrl);

    Ok(bytes)
}

/// Open a passive data connection via EPSV (preferred) or PASV (fallback).
/// Returns the `(host, port)` we should dial for the data channel.
///
/// EPSV reply form: `229 Entering Extended Passive Mode (|||port|)`.
/// PASV reply form: `227 Entering Passive Mode (h1,h2,h3,h4,p1,p2)`.
fn open_passive<R: Read + Write>(
    ctrl: &mut BufReader<R>,
    fallback_host: &str,
    _fallback_port: u16,
) -> Result<(String, u16)> {
    send(ctrl, "EPSV")?;
    let (c, m) = read_reply(ctrl)?;
    if c == 229 {
        let port = parse_epsv(&m)
            .ok_or_else(|| Error::BadResponse(format!("ftp EPSV: cannot parse reply: {m}")))?;
        // EPSV doesn't carry a host; reuse the control connection's host
        // (which is also what curl/RFC 2428 says clients should do).
        return Ok((fallback_host.to_string(), port));
    }
    // 5xx → not supported, try PASV. 4xx → transient, but we still try
    // PASV: nothing in the EPSV failure precludes PASV working.
    if !(400..600).contains(&c) {
        return Err(Error::BadResponse(format!("ftp EPSV: {c} {m}")));
    }
    send(ctrl, "PASV")?;
    let (c2, m2) = read_reply(ctrl)?;
    if c2 != 227 {
        return Err(Error::BadResponse(format!("ftp PASV: {c2} {m2}")));
    }
    parse_pasv(&m2).ok_or_else(|| Error::BadResponse(format!("ftp PASV: cannot parse: {m2}")))
}

/// Write a single FTP command followed by CRLF, using the BufReader's
/// underlying writer (BufReader itself isn't `Write`).
fn send<R: Read + Write>(r: &mut BufReader<R>, line: &str) -> Result<()> {
    let w = r.get_mut();
    w.write_all(line.as_bytes())?;
    w.write_all(b"\r\n")?;
    w.flush()?;
    Ok(())
}

/// Read a (possibly multi-line) FTP reply. Returns `(code, text)` where
/// `text` is the concatenation of every line's text portion separated by
/// `\n`, without the trailing CRLF.
///
/// Multi-line replies look like:
///   `NNN-first line\r\n`
///   `   continuation\r\n`
///   `NNN final line\r\n`
/// — i.e. the terminator is a line whose first four bytes are `NNN` + ' '.
fn read_reply<R: BufRead>(r: &mut R) -> Result<(u16, String)> {
    let first = read_line(r)?;
    let (code, sep, rest) = split_code(&first)?;
    let mut text = rest.to_string();
    if sep == ' ' {
        return Ok((code, text));
    }
    // sep == '-': multi-line continuation until "<code> ..." is seen.
    loop {
        let line = read_line(r)?;
        // A continuation line may or may not start with the code. The
        // terminator is specifically `NNN ` (code + space).
        if let Ok((c, s, rest)) = split_code(&line) {
            text.push('\n');
            text.push_str(rest);
            if c == code && s == ' ' {
                return Ok((code, text));
            }
        } else {
            text.push('\n');
            text.push_str(line.trim_end_matches(['\r', '\n']));
        }
    }
}

/// Read one CRLF-terminated line, stripping the trailing CRLF. EOF before
/// any newline is an error.
fn read_line<R: BufRead>(r: &mut R) -> Result<String> {
    let mut buf = String::new();
    let n = r.read_line(&mut buf)?;
    if n == 0 {
        return Err(Error::UnexpectedEof);
    }
    Ok(buf)
}

/// Parse the leading 3-digit code from an FTP reply line. Returns
/// `(code, separator, rest)` where separator is ' ' (final line) or '-'
/// (continuation).
fn split_code(line: &str) -> Result<(u16, char, &str)> {
    let bytes = line.as_bytes();
    if bytes.len() < 4
        || !bytes[0].is_ascii_digit()
        || !bytes[1].is_ascii_digit()
        || !bytes[2].is_ascii_digit()
    {
        return Err(Error::BadResponse(format!(
            "ftp reply: no 3-digit code: {}",
            line.trim_end()
        )));
    }
    let sep = bytes[3] as char;
    if sep != ' ' && sep != '-' {
        return Err(Error::BadResponse(format!(
            "ftp reply: bad separator: {}",
            line.trim_end()
        )));
    }
    let code: u16 = line[..3].parse().unwrap(); // ascii_digit-checked above
    let rest = line[4..].trim_end_matches(['\r', '\n']);
    Ok((code, sep, rest))
}

/// Parse the `(h1,h2,h3,h4,p1,p2)` payload of a 227 PASV reply and turn it
/// into a `"a.b.c.d", port` pair. Returns `None` if the reply isn't shaped
/// the way the spec says.
fn parse_pasv(text: &str) -> Option<(String, u16)> {
    let open = text.find('(')?;
    let close = text[open..].find(')')? + open;
    let inner = &text[open + 1..close];
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 6 {
        return None;
    }
    let nums: Vec<u16> = parts.iter().filter_map(|p| p.parse::<u16>().ok()).collect();
    if nums.len() != 6 || nums[..4].iter().any(|&n| n > 255) {
        return None;
    }
    let host = format!("{}.{}.{}.{}", nums[0], nums[1], nums[2], nums[3]);
    let port = (nums[4] << 8) | nums[5];
    Some((host, port))
}

/// Parse the `(|||port|)` payload of a 229 EPSV reply. The single delimiter
/// character (here `|`) is chosen by the server and may differ — we use the
/// character immediately after `(`.
fn parse_epsv(text: &str) -> Option<u16> {
    let open = text.find('(')?;
    let close = text[open..].rfind(')')? + open;
    let inner = text.get(open + 1..close)?;
    // First byte is the delimiter (must be the same char repeated 3 times,
    // then the port, then the same delimiter again).
    let mut chars = inner.chars();
    let delim = chars.next()?;
    // Find the 3rd delim from the start; everything between it and the 4th
    // is the port.
    let bytes: Vec<char> = inner.chars().collect();
    let mut count = 0usize;
    let mut start = None;
    let mut end = None;
    for (i, ch) in bytes.iter().enumerate() {
        if *ch == delim {
            count += 1;
            if count == 3 {
                start = Some(i + 1);
            } else if count == 4 {
                end = Some(i);
                break;
            }
        }
    }
    let s = start?;
    let e = end?;
    let port_str: String = bytes[s..e].iter().collect();
    port_str.parse().ok()
}

/// Split `user[:pass]` into `(user, pass)`, defaulting to anonymous /
/// `rsurl@` (matching real curl's anonymous-FTP defaults).
fn split_userinfo(ui: Option<&str>) -> (String, String) {
    match ui {
        None => ("anonymous".to_string(), "rsurl@".to_string()),
        Some(s) => match s.split_once(':') {
            Some((u, p)) => (u.to_string(), p.to_string()),
            None => (s.to_string(), "rsurl@".to_string()),
        },
    }
}

/// 2xx and 3xx are "positive" reply categories (completion / intermediate).
fn is_positive(code: u16) -> bool {
    (200..400).contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn cur(s: &str) -> BufReader<Cursor<Vec<u8>>> {
        BufReader::new(Cursor::new(s.as_bytes().to_vec()))
    }

    #[test]
    fn read_reply_single_line() {
        let mut r = cur("220 ProFTPD ready\r\n");
        let (code, text) = read_reply(&mut r).unwrap();
        assert_eq!(code, 220);
        assert_eq!(text, "ProFTPD ready");
    }

    #[test]
    fn read_reply_multi_line() {
        // RFC 959 §4.2 example shape. Continuation lines may start with
        // the same code or with arbitrary text; the terminator is `NNN `.
        let raw = "220-Welcome to the FTP server\r\n\
                   220-We have rules\r\n\
                   220 End of banner\r\n";
        let mut r = cur(raw);
        let (code, text) = read_reply(&mut r).unwrap();
        assert_eq!(code, 220);
        assert!(text.contains("Welcome"));
        assert!(text.contains("End of banner"));
    }

    #[test]
    fn read_reply_multi_line_continuation_without_code() {
        // Some servers emit continuation lines that don't start with the
        // code at all. Make sure we keep reading until `NNN `.
        let raw = "230-User logged in\r\n   please read MOTD\r\n230 ok\r\n";
        let mut r = cur(raw);
        let (code, text) = read_reply(&mut r).unwrap();
        assert_eq!(code, 230);
        assert!(text.contains("User logged in"));
        assert!(text.contains("please read MOTD"));
        assert!(text.contains("ok"));
    }

    #[test]
    fn read_reply_eof_is_error() {
        let mut r = cur("");
        assert!(matches!(read_reply(&mut r), Err(Error::UnexpectedEof)));
    }

    #[test]
    fn read_reply_rejects_garbage() {
        let mut r = cur("hello world\r\n");
        assert!(matches!(read_reply(&mut r), Err(Error::BadResponse(_))));
    }

    #[test]
    fn pasv_parses_canonical() {
        let (host, port) = parse_pasv("Entering Passive Mode (10,0,0,1,4,5)").unwrap();
        assert_eq!(host, "10.0.0.1");
        assert_eq!(port, 4 * 256 + 5); // 1029
    }

    #[test]
    fn pasv_parses_with_prefix_code_text() {
        // We pass `parse_pasv` only the text part (no code), matching how
        // `read_reply` returns things.
        let (host, port) = parse_pasv("Entering Passive Mode (192,168,1,2,200,100).").unwrap();
        assert_eq!(host, "192.168.1.2");
        assert_eq!(port, 200 * 256 + 100);
    }

    #[test]
    fn pasv_rejects_short() {
        assert!(parse_pasv("nope").is_none());
        assert!(parse_pasv("(1,2,3)").is_none());
        assert!(parse_pasv("(256,0,0,1,1,1)").is_none()); // octet > 255
    }

    #[test]
    fn epsv_parses_canonical() {
        let port = parse_epsv("Entering Extended Passive Mode (|||45678|)").unwrap();
        assert_eq!(port, 45678);
    }

    #[test]
    fn epsv_parses_alternative_delimiter() {
        // RFC 2428 lets the server pick any delimiter; we just read the
        // first char after '('.
        let port = parse_epsv("(!!!2121!)").unwrap();
        assert_eq!(port, 2121);
    }

    #[test]
    fn epsv_rejects_garbage() {
        assert!(parse_epsv("nope").is_none());
        assert!(parse_epsv("(|||abc|)").is_none());
    }

    #[test]
    fn split_userinfo_defaults_to_anonymous() {
        let (u, p) = split_userinfo(None);
        assert_eq!(u, "anonymous");
        assert_eq!(p, "rsurl@");
    }

    #[test]
    fn split_userinfo_user_only() {
        let (u, p) = split_userinfo(Some("alice"));
        assert_eq!(u, "alice");
        assert_eq!(p, "rsurl@");
    }

    #[test]
    fn split_userinfo_user_pass() {
        let (u, p) = split_userinfo(Some("alice:secret"));
        assert_eq!(u, "alice");
        assert_eq!(p, "secret");
    }

    #[test]
    fn split_userinfo_pass_with_colon() {
        let (u, p) = split_userinfo(Some("alice:s:e:c"));
        assert_eq!(u, "alice");
        assert_eq!(p, "s:e:c");
    }

    #[test]
    fn split_code_parses_space_and_dash() {
        let (c, s, r) = split_code("200 OK\r\n").unwrap();
        assert_eq!(c, 200);
        assert_eq!(s, ' ');
        assert_eq!(r, "OK");

        let (c, s, r) = split_code("220-banner\r\n").unwrap();
        assert_eq!(c, 220);
        assert_eq!(s, '-');
        assert_eq!(r, "banner");
    }

    #[test]
    fn fetch_rejects_non_ftp_scheme() {
        let u = Url::parse("http://example.com/").unwrap();
        assert!(matches!(fetch(&u), Err(Error::UnsupportedScheme(_))));
    }
}
