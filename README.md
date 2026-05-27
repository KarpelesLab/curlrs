# curlrs

A pure-Rust implementation of curl, built on top of [purecrypto](https://crates.io/crates/purecrypto)
for TLS — no OpenSSL, no system libcurl, no C dependencies.

`curlrs` ships in three forms:

1. **Rust library** (`curlrs` crate) — a small, ergonomic HTTP client API for Rust projects.
2. **C library** (`libcurlrs.so` / `curlrs.h`) — a curl-compatible C ABI for non-Rust consumers.
3. **`curlrs` CLI** — a drop-in-ish replacement for the `curl` command line.

## Status

Early, in active development.

| Capability | Status |
|---|---|
| HTTP/1.1 GET | working |
| HTTP/1.1 other methods | working (basic) |
| HTTPS via purecrypto | working (TLS 1.2/1.3, system roots, cert verification) |
| FTP / FTPS | planned |
| HTTP/2, HTTP/3 | not yet |

System CA bundle paths searched, in order: `/etc/ssl/certs/ca-certificates.crt`,
`/etc/pki/tls/certs/ca-bundle.crt`, `/etc/ssl/cert.pem`, `/etc/ssl/ca-bundle.pem`,
`/etc/ca-certificates/extracted/tls-ca-bundle.pem`.

## Rust usage

```rust
let resp = curlrs::get("http://example.com")?;
println!("{} {}", resp.status, resp.reason);
println!("{}", String::from_utf8_lossy(&resp.body));
```

## CLI usage

```sh
curlrs http://example.com
curlrs -o out.html -v http://example.com
```

## C usage

```c
#include "curlrs.h"

CURLRS *h = curlrs_easy_init();
curlrs_easy_setopt_str(h, CURLRSOPT_URL, "http://example.com");
curlrs_easy_perform(h);

const uint8_t *body; size_t len;
curlrs_easy_response_body(h, &body, &len);
printf("%ld %.*s\n", curlrs_easy_response_status(h), (int)len, body);

curlrs_easy_cleanup(h);
```

Link with `-lcurlrs`. Function names use a `curlrs_` prefix so the library
can coexist with libcurl in the same process.

## Build

```sh
cargo build --release
# Binary:       target/release/curlrs
# Rust rlib:    target/release/libcurlrs.rlib
# C cdylib:     target/release/libcurlrs.so
# C header:     include/curlrs.h
```

## License

MIT — Copyright © 2026 Karpelès Lab Inc. See [LICENSE](LICENSE).
