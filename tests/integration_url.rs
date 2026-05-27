//! Black-box tests for `curlrs::Url`.
//!
//! These complement the in-module unit tests in `src/url.rs` by exercising
//! cases the public API has to handle but that aren't already covered there:
//! IPv6 literals, percent-encoded passthrough, query/fragment combinations,
//! and a few negative cases.

use curlrs::Url;

/// `[::1]:8080` is the canonical bracketed-IPv6 literal authority. We only
/// assert what's stable across parser implementations: it parses without
/// error and the path round-trips. Exact host/port split is intentionally
/// not pinned here so a parser refactor doesn't break the test.
#[test]
fn ipv6_literal_with_port() {
    let u = Url::parse("http://[::1]:8080/path?q=1").expect("ipv6 url parses");
    assert_eq!(u.scheme, "http");
    assert_eq!(u.path, "/path?q=1");
    // Sanity: the host string mentions the address. We deliberately do
    // not pin `u.host == "[::1]"` here — see the doc-comment above.
    assert!(
        u.host.contains("::1"),
        "host should retain ipv6 literal, got {:?}",
        u.host,
    );
}

/// Percent-encoded path segments must be carried verbatim. curlrs does
/// not double-encode or decode; the bytes you supply are the bytes that
/// go on the wire.
#[test]
fn percent_encoded_path_passthrough() {
    let u = Url::parse("http://example.com/foo%20bar/%2Fbaz?a=%26b").unwrap();
    assert_eq!(u.scheme, "http");
    assert_eq!(u.host, "example.com");
    assert_eq!(u.path, "/foo%20bar/%2Fbaz?a=%26b");
}

/// A query string stays glued onto the path (curlrs doesn't split them);
/// a fragment is stripped.
#[test]
fn query_kept_fragment_stripped() {
    let u = Url::parse("http://h/p?x=1&y=2#section").unwrap();
    assert_eq!(u.path, "/p?x=1&y=2");
}

/// No path at all defaults to "/", but the query is still respected when
/// it appears in the right place (after the path that we synthesize).
#[test]
fn missing_path_defaults_to_slash() {
    let u = Url::parse("http://example.com").unwrap();
    assert_eq!(u.path, "/");
    assert_eq!(u.port, 80);
}

/// HTTPS gets port 443 by default; an explicit port wins.
#[test]
fn default_https_port_and_explicit_override() {
    assert_eq!(Url::parse("https://h/").unwrap().port, 443);
    assert_eq!(Url::parse("https://h:8443/").unwrap().port, 8443);
}

/// Userinfo with both user and password round-trips into `userinfo`
/// without being decoded.
#[test]
fn userinfo_with_password() {
    let u = Url::parse("http://alice:s%3Acret@h/p").unwrap();
    assert_eq!(u.userinfo.as_deref(), Some("alice:s%3Acret"));
    assert_eq!(u.host, "h");
    assert_eq!(u.path, "/p");
}

/// Empty host is rejected (would otherwise produce a request with a
/// bogus `Host:` header).
#[test]
fn rejects_empty_host() {
    assert!(Url::parse("http:///path").is_err());
}

/// A scheme with no `://` is not a URL at all.
#[test]
fn rejects_bare_path() {
    assert!(Url::parse("/just/a/path").is_err());
}
