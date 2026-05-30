# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.2](https://github.com/KarpelesLab/rsurl/compare/v0.0.1...v0.0.2) - 2026-05-30

### Other

- Merge branch 'worktree-agent-a8eba2346aa84a9ee'
- Merge branch 'worktree-agent-a8a6c125f123e957a'
- Merge branch 'worktree-agent-a303cb1d088f59815'
- Merge branch 'worktree-agent-a37dd55d6037057b6'
- Merge branch 'worktree-agent-addf977f176bb0950'
- fill the curl-parity body-flag coverage gaps
- -F multipart, --form-string, --form-escape, -T upload
- --data-binary, --data-urlencode, --data-raw + repeatable -d
- swap flate2 for compcol (our pure-Rust codec collection)
- fix broken intra-doc link to `load_netscape`
- force blocking I/O on accepted sockets (Windows fix)
- Add HTTP/1.1 connection-reuse pool with stale-connection retry
- Add HTTP proxy support: -x/--proxy, --proxy-user, --noproxy
- Add cookie jar (-b / -c) compatible with curl's Netscape format
- Decode `Content-Encoding: gzip|deflate` responses transparently
- Add `rustls-tls` Cargo feature as an alternative TLS backend
- HPACK encoder Huffman + dynamic-table insertion (RFC 7541 §5.2, §6.2.1, §6.3)
- process-wide connection pool keyed on (scheme, host, port)
- stream multiplexing + per-stream state machine (RFC 9113 §5.1)
- CONTINUATION on encode + DATA fragmentation with flow-control gating
- implement connection + stream flow control (RFC 9113 §6.9)
- parse and apply peer SETTINGS (RFC 9113 §6.5)
- graceful TCP close in the integration test server
- Color MIT license badge blue
- Add CI / crates.io / docs.rs / MIT badges to README
- rustfmt cleanup of rsurl_easy_response_header signature
- Add staticlib to crate-type so cargo build produces librsurl.a

### Security

- cap HTTP/2 body and header-block growth, enforce header-list limits, thread h3 TLS opts
