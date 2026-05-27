use crate::error::{Error, Result};

/// Minimal parsed URL. Only the fields we need for the protocols we speak.
///
/// Userinfo (user:pass@) is captured in `userinfo` for protocols that need
/// auth, but not percent-decoded. Fragments are stripped. Query strings stay
/// attached to `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    /// `user[:pass]` from before the `@` in the authority, if present.
    pub userinfo: Option<String>,
    pub host: String,
    pub port: u16,
    /// Path including the query string, always starting with `/` (except for
    /// schemes like `dict:` and `gopher:` where the path can be a single
    /// token). For `file://` URLs, this is the absolute filesystem path.
    pub path: String,
}

impl Url {
    pub fn parse(s: &str) -> Result<Self> {
        let (scheme, rest) = s
            .split_once("://")
            .ok_or_else(|| Error::InvalidUrl(s.to_string()))?;
        if scheme.is_empty() {
            return Err(Error::InvalidUrl(s.to_string()));
        }
        let scheme = scheme.to_ascii_lowercase();

        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };

        // `file://` is special: no host, the path is everything after `file://`.
        if scheme == "file" {
            let path = match s.strip_prefix("file://") {
                Some(p) if p.starts_with('/') => p.to_string(),
                Some(_) | None => return Err(Error::InvalidUrl(s.to_string())),
            };
            let path = match path.find('#') {
                Some(i) => path[..i].to_string(),
                None => path,
            };
            return Ok(Url {
                scheme,
                userinfo: None,
                host: String::new(),
                port: 0,
                path,
            });
        }

        if authority.is_empty() {
            return Err(Error::InvalidUrl(s.to_string()));
        }

        // Strip optional fragment from path.
        let path = match path.find('#') {
            Some(i) => &path[..i],
            None => path,
        };

        let default_port =
            default_port(&scheme).ok_or_else(|| Error::UnsupportedScheme(scheme.clone()))?;

        let (userinfo, hostport) = match authority.rfind('@') {
            Some(i) => (Some(authority[..i].to_string()), &authority[i + 1..]),
            None => (None, authority),
        };

        let (host, port) = match hostport.rfind(':') {
            Some(i) if !hostport[..i].contains(']') => {
                let h = &hostport[..i];
                let p: u16 = hostport[i + 1..]
                    .parse()
                    .map_err(|_| Error::InvalidUrl(s.to_string()))?;
                (h, p)
            }
            _ => (hostport, default_port),
        };

        if host.is_empty() {
            return Err(Error::InvalidUrl(s.to_string()));
        }

        Ok(Url {
            scheme,
            userinfo,
            host: host.to_string(),
            port,
            path: path.to_string(),
        })
    }

    /// True if this scheme runs over TLS at the transport layer.
    pub fn is_tls(&self) -> bool {
        matches!(
            self.scheme.as_str(),
            "https" | "ftps" | "imaps" | "pop3s" | "ldaps" | "gophers" | "mqtts" | "wss"
        )
    }
}

/// Default port for every scheme curlrs knows about. Returning `None` means
/// the scheme is not recognized at all (URL parsing will reject it).
fn default_port(scheme: &str) -> Option<u16> {
    Some(match scheme {
        "http" | "ws" => 80,
        "https" | "wss" => 443,
        "ftp" => 21,
        "ftps" => 990,
        "dict" => 2628,
        "gopher" | "gophers" => 70,
        "imap" => 143,
        "imaps" => 993,
        "ldap" => 389,
        "ldaps" => 636,
        "mqtt" => 1883,
        "mqtts" => 8883,
        "pop3" => 110,
        "pop3s" => 995,
        "rtsp" => 554,
        "tftp" => 69,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http() {
        let u = Url::parse("http://example.com/foo?bar=1").unwrap();
        assert_eq!(u.scheme, "http");
        assert_eq!(u.host, "example.com");
        assert_eq!(u.port, 80);
        assert_eq!(u.path, "/foo?bar=1");
        assert_eq!(u.userinfo, None);
    }

    #[test]
    fn parses_https_with_port() {
        let u = Url::parse("https://example.com:8443").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.port, 8443);
        assert_eq!(u.path, "/");
    }

    #[test]
    fn rejects_no_scheme() {
        assert!(Url::parse("example.com").is_err());
    }

    #[test]
    fn strips_fragment() {
        let u = Url::parse("http://x/y#frag").unwrap();
        assert_eq!(u.path, "/y");
    }

    #[test]
    fn parses_userinfo() {
        let u = Url::parse("ftp://alice:secret@ftp.example.com/pub/").unwrap();
        assert_eq!(u.scheme, "ftp");
        assert_eq!(u.userinfo.as_deref(), Some("alice:secret"));
        assert_eq!(u.host, "ftp.example.com");
        assert_eq!(u.port, 21);
        assert_eq!(u.path, "/pub/");
    }

    #[test]
    fn parses_file_url() {
        let u = Url::parse("file:///etc/hosts").unwrap();
        assert_eq!(u.scheme, "file");
        assert_eq!(u.host, "");
        assert_eq!(u.path, "/etc/hosts");
    }

    #[test]
    fn default_ports_cover_all_protocols() {
        for scheme in [
            "http", "https", "ftp", "ftps", "dict", "gopher", "gophers", "imap", "imaps", "ldap",
            "ldaps", "mqtt", "mqtts", "pop3", "pop3s", "rtsp", "tftp", "ws", "wss",
        ] {
            let url = format!("{scheme}://example.com");
            let u = Url::parse(&url).unwrap_or_else(|e| panic!("scheme {scheme}: {e}"));
            assert_ne!(u.port, 0, "scheme {scheme} got port 0");
        }
    }

    #[test]
    fn is_tls_classification() {
        for s in [
            "https", "ftps", "imaps", "pop3s", "ldaps", "gophers", "mqtts", "wss",
        ] {
            let u = Url::parse(&format!("{s}://h")).unwrap();
            assert!(u.is_tls(), "{s} should be tls");
        }
        for s in [
            "http", "ftp", "imap", "pop3", "ldap", "gopher", "mqtt", "ws", "dict", "tftp", "rtsp",
        ] {
            let u = Url::parse(&format!("{s}://h")).unwrap();
            assert!(!u.is_tls(), "{s} should not be tls");
        }
    }
}
