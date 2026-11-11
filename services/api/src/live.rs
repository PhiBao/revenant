//! Live mode: restore a real page from the public web archive.
//!
//! See requirement group L in `.kiro/specs/resurrection-api/requirements.md`.
//!
//! The URL a visitor types is never fetched. Its host and path are only sent, percent-encoded, as a
//! query parameter to the archive's own index. Every outgoing request goes to the one archive host
//! in the configuration.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use revenant_core::fidelity::Capture;
use revenant_core::pipeline::{Lookup, Mode, SnapshotSource, SourceError};
use revenant_core::time::parse_cdx_timestamp;

pub const MAX_URL_BYTES: usize = 2048;
pub const MAX_BODY_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_LIVE_ASSETS: usize = 40;
pub const MAX_LIVE_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const USER_AGENT: &str =
    "Revenant/0.1 (+https://github.com/PhiBao/revenant; polite single-file restorer)";

// ---------------------------------------------------------------- URL validation (L1.3, L1.4)

/// The part of a visitor's URL that Revenant uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveTarget {
    /// Bare host without `www.`, for example `y2k.gov`.
    pub site: String,
    /// Path without the leading slash. Empty means the site root.
    pub page: String,
    /// True when a query string or fragment was present and ignored.
    pub dropped_query: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlError(pub &'static str);

pub fn parse_target(raw: &str) -> Result<LiveTarget, UrlError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(UrlError("Enter the address of a page."));
    }
    if raw.len() > MAX_URL_BYTES {
        return Err(UrlError("That address is too long."));
    }
    if raw.bytes().any(|b| !(0x21..=0x7e).contains(&b)) {
        return Err(UrlError("Use a plain ASCII address without spaces."));
    }

    let lower = raw.to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &raw[8..]
    } else if lower.starts_with("http://") {
        &raw[7..]
    } else if raw.contains("://") {
        return Err(UrlError("Only http and https addresses are supported."));
    } else {
        raw
    };

    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let tail = &rest[authority_end..];

    if authority.contains('@') {
        return Err(UrlError(
            "Addresses with a user name or password are not supported.",
        ));
    }
    if authority.starts_with('[') {
        return Err(UrlError(
            "IP addresses are not supported. Use the site's name.",
        ));
    }
    let host_part = match authority.split_once(':') {
        Some((h, port)) => {
            if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
                return Err(UrlError("That address has an invalid port."));
            }
            h
        }
        None => authority,
    };
    let host = host_part.trim_end_matches('.').to_ascii_lowercase();
    validate_host(&host)?;

    let dropped_query = tail.contains('?') || tail.contains('#');
    let path_end = tail.find(['?', '#']).unwrap_or(tail.len());
    let path = tail[..path_end].trim_start_matches('/');

    if path.contains("//") || path.contains('\\') {
        return Err(UrlError("That address has an invalid path."));
    }
    if path.split('/').any(|seg| seg == "." || seg == "..") {
        return Err(UrlError("That address has an invalid path."));
    }
    if !path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"._~%/+,;=@!-".contains(&b))
    {
        return Err(UrlError(
            "That address has characters Revenant does not accept.",
        ));
    }
    if path.ends_with('/') {
        return Err(UrlError(
            "Give the address of a page, not a folder. Use the root address, or a file such as /about.html.",
        ));
    }

    let site = host.strip_prefix("www.").unwrap_or(&host).to_string();
    Ok(LiveTarget {
        site,
        page: path.to_string(),
        dropped_query,
    })
}

fn validate_host(host: &str) -> Result<(), UrlError> {
    if host.is_empty() {
        return Err(UrlError("That address has no site name."));
    }
    if host.len() > 253 {
        return Err(UrlError("That site name is too long."));
    }
    if host == "localhost" || !host.contains('.') {
        return Err(UrlError("Use a full site name such as example.org."));
    }
    let labels: Vec<&str> = host.split('.').collect();
    for label in &labels {
        let ok = !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !ok {
            return Err(UrlError("That is not a valid site name."));
        }
    }
    // An all-numeric last label is an IP address, never a real top-level domain.
    if labels
        .last()
        .is_some_and(|l| l.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(UrlError(
            "IP addresses are not supported. Use the site's name.",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------- archive client (L3)

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable(pub String);

/// Keeps at least `gap` between archive requests, across every run.
pub struct Pacer {
    gap: Duration,
    next: Mutex<Instant>,
}

impl Pacer {
    pub fn new(gap: Duration) -> Self {
        Self {
            gap,
            next: Mutex::new(Instant::now()),
        }
    }

    pub fn wait(&self) {
        let mut next = self.next.lock().expect("pacer lock");
        let now = Instant::now();
        if *next > now {
            std::thread::sleep(*next - now);
        }
        *next = Instant::now() + self.gap;
    }
}

/// One shared client for the process. It only ever talks to `base`.
pub struct ArchiveClient {
    base: String,
    host: String,
    agent: ureq::Agent,
    pacer: Pacer,
}

impl ArchiveClient {
    pub fn new(base: &str, gap: Duration) -> Result<Self, String> {
        let base = base.trim_end_matches('/').to_string();
        let host = base
            .split("://")
            .nth(1)
            .and_then(|r| r.split('/').next())
            .filter(|h| !h.is_empty())
            .ok_or_else(|| format!("bad archive base url: {base}"))?
            .to_string();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(25)))
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self {
            base,
            host,
            agent,
            pacer: Pacer::new(gap),
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    /// GET a URL on the archive host. Follows at most three redirects, all within the host.
    fn get(&self, path_and_query: &str, limit: u64) -> Result<Vec<u8>, Unavailable> {
        let mut url = format!("{}{}", self.base, path_and_query);
        let mut retried = false;
        let mut hops = 0;
        loop {
            self.pacer.wait();
            let mut resp = self
                .agent
                .get(&url)
                .call()
                .map_err(|e| Unavailable(format!("network error: {e}")))?;
            let status = resp.status().as_u16();
            match status {
                200..=299 => {
                    return resp
                        .body_mut()
                        .with_config()
                        .limit(limit)
                        .read_to_vec()
                        .map_err(|e| Unavailable(format!("could not read the response: {e}")));
                }
                301 | 302 | 303 | 307 | 308 => {
                    hops += 1;
                    if hops > 3 {
                        return Err(Unavailable("too many redirects".into()));
                    }
                    let location = resp
                        .headers()
                        .get("location")
                        .and_then(|v| v.to_str().ok())
                        .ok_or_else(|| Unavailable("redirect without a location".into()))?
                        .to_string();
                    url = self.same_host(&url, &location)?;
                }
                429 | 500..=599 if !retried => {
                    retried = true;
                    let wait = resp
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .unwrap_or(2)
                        .min(10);
                    std::thread::sleep(Duration::from_secs(wait));
                }
                403 => {
                    return Err(Unavailable(
                        "the archive refused the request (HTTP 403)".into(),
                    ))
                }
                404 => {
                    return Err(Unavailable(
                        "the archive has no replay for it (HTTP 404)".into(),
                    ))
                }
                other => return Err(Unavailable(format!("the archive answered HTTP {other}"))),
            }
        }
    }

    /// Resolve a Location header and refuse anything that leaves the archive host (L3.2).
    fn same_host(&self, current: &str, location: &str) -> Result<String, Unavailable> {
        if let Some(rest) = location.strip_prefix('/') {
            if rest.starts_with('/') {
                return Err(Unavailable("redirect to another host refused".into()));
            }
            return Ok(format!("{}/{}", self.base, rest));
        }
        let scheme_end = location
            .find("://")
            .ok_or_else(|| Unavailable("unsupported redirect".into()))?;
        let after = &location[scheme_end + 3..];
        let host = after.split(['/', '?', '#']).next().unwrap_or("");
        if host != self.host {
            return Err(Unavailable("redirect to another host refused".into()));
        }
        let _ = current;
        // Rebuild on our own base so the scheme cannot be downgraded or changed.
        let path = &after[host.len()..];
        Ok(format!("{}{}", self.base, path))
    }

    fn cdx(&self, key: &str, window: &str, limit: &str) -> Result<Vec<CdxRow>, Unavailable> {
        let q = format!(
            "/cdx/search/cdx?url={}&output=json&fl=original,timestamp,digest&filter=statuscode:200&{window}&limit={limit}",
            encode_component(key)
        );
        let body = self.get(&q, 2 * 1024 * 1024)?;
        parse_cdx(&body)
    }

    fn replay(&self, timestamp: &str, original: &str) -> Result<Vec<u8>, Unavailable> {
        // The original comes from the archive's own index, never from the visitor.
        self.get(&format!("/web/{timestamp}id_/{original}"), MAX_BODY_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdxRow {
    pub original: String,
    pub timestamp: String,
    pub digest: String,
}

pub fn parse_cdx(body: &[u8]) -> Result<Vec<CdxRow>, Unavailable> {
    let text = std::str::from_utf8(body).map_err(|_| Unavailable("index was not text".into()))?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| Unavailable("index answer was not understood".into()))?;
    let rows = value
        .as_array()
        .ok_or_else(|| Unavailable("index answer was not a list".into()))?;
    let mut out = Vec::new();
    for row in rows.iter().skip(1) {
        let cols = row
            .as_array()
            .filter(|c| c.len() >= 3)
            .ok_or_else(|| Unavailable("index row was malformed".into()))?;
        let get = |i: usize| cols[i].as_str().map(str::to_string);
        match (get(0), get(1), get(2)) {
            (Some(original), Some(timestamp), Some(digest)) => out.push(CdxRow {
                original,
                timestamp,
                digest,
            }),
            _ => return Err(Unavailable("index row was malformed".into())),
        }
    }
    Ok(out)
}

/// Percent-encode everything except RFC 3986 unreserved characters.
pub fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ---------------------------------------------------------------- live source

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub stage: String,
    pub requests: u32,
}

type BodyCache = Mutex<HashMap<(String, i64), Result<Vec<u8>, SourceError>>>;

/// A `SnapshotSource` backed by the live archive. One per run. It caches every answer, so the
/// strict and the nearest-capture restore of a run cost the archive one set of requests (L3.4).
pub struct LiveSource {
    client: Arc<ArchiveClient>,
    target_ts: i64,
    progress: Arc<Mutex<Progress>>,
    lookups: Mutex<HashMap<String, Lookup>>,
    originals: Mutex<HashMap<(String, i64), (String, String)>>,
    bodies: BodyCache,
}

impl LiveSource {
    pub fn new(client: Arc<ArchiveClient>, target_ts: i64, progress: Arc<Mutex<Progress>>) -> Self {
        Self {
            client,
            target_ts,
            progress,
            lookups: Mutex::new(HashMap::new()),
            originals: Mutex::new(HashMap::new()),
            bodies: Mutex::new(HashMap::new()),
        }
    }

    fn note(&self, stage: String) {
        let mut p = self.progress.lock().expect("progress lock");
        p.stage = stage;
        p.requests += 1;
    }

    fn query(&self, label: &str) -> Lookup {
        self.note(format!("Asking the archive about {label}"));
        let before_window = format!(
            "to={}",
            revenant_core::time::format_cdx_timestamp(self.target_ts)
        );
        let after_window = format!(
            "from={}",
            revenant_core::time::format_cdx_timestamp(self.target_ts + 1)
        );
        let before = match self.client.cdx(label, &before_window, "-1") {
            Ok(r) => r,
            Err(e) => return Lookup::Unknown(e.0),
        };
        let after = match self.client.cdx(label, &after_window, "1") {
            Ok(r) => r,
            Err(e) => return Lookup::Unknown(e.0),
        };

        let mut caps: Vec<Capture> = Vec::new();
        let mut originals = self.originals.lock().expect("originals lock");
        for row in before.into_iter().chain(after) {
            let Some(ts) = parse_cdx_timestamp(&row.timestamp) else {
                continue;
            };
            if caps.iter().any(|c| c.ts == ts) {
                continue;
            }
            originals.insert(
                (label.to_string(), ts),
                (row.timestamp.clone(), row.original.clone()),
            );
            caps.push(Capture {
                ts,
                digest: row.digest,
                content: None,
            });
        }
        Lookup::Captures(caps)
    }
}

impl SnapshotSource for LiveSource {
    fn mode(&self) -> Mode {
        Mode::Live
    }

    fn archive_name(&self) -> &str {
        self.client.host()
    }

    fn lookup(&self, label: &str) -> Lookup {
        if let Some(hit) = self.lookups.lock().expect("lookup cache lock").get(label) {
            return hit.clone();
        }
        let result = self.query(label);
        self.lookups
            .lock()
            .expect("lookup cache lock")
            .insert(label.to_string(), result.clone());
        result
    }

    fn body(&self, label: &str, capture: &Capture) -> Result<Vec<u8>, SourceError> {
        let key = (label.to_string(), capture.ts);
        if let Some(hit) = self.bodies.lock().expect("body cache lock").get(&key) {
            return hit.clone();
        }
        let (stamp, original) = self
            .originals
            .lock()
            .expect("originals lock")
            .get(&key)
            .cloned()
            .ok_or_else(|| SourceError(format!("no index entry for {label}")))?;
        self.note(format!("Fetching {label}"));
        let result = self
            .client
            .replay(&stamp, &original)
            .map_err(|e| SourceError(e.0));
        self.bodies
            .lock()
            .expect("body cache lock")
            .insert(key, result.clone());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(raw: &str) -> LiveTarget {
        parse_target(raw).unwrap_or_else(|e| panic!("{raw}: {}", e.0))
    }

    fn bad(raw: &str) {
        assert!(parse_target(raw).is_err(), "{raw} was accepted");
    }

    #[test]
    fn accepts_ordinary_addresses() {
        assert_eq!(ok("http://www.y2k.gov/briefings.html").site, "y2k.gov");
        assert_eq!(
            ok("http://www.y2k.gov/briefings.html").page,
            "briefings.html"
        );
        assert_eq!(ok("https://Example.ORG").page, "");
        assert_eq!(ok("example.org/about.html").site, "example.org");
        assert_eq!(ok("http://example.org:80/a/b.html").page, "a/b.html");
        assert_eq!(ok("http://example.org./x.html").site, "example.org");
    }

    #[test]
    fn query_and_fragment_are_dropped_and_reported() {
        let t = ok("http://example.org/page.html?id=7#top");
        assert_eq!(t.page, "page.html");
        assert!(t.dropped_query);
        assert!(!ok("http://example.org/page.html").dropped_query);
    }

    #[test]
    fn rejects_other_schemes_and_odd_hosts() {
        for raw in [
            "",
            "   ",
            "ftp://example.org/x",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "http://user:pw@example.org/",
            "http://localhost/",
            "http://intranet/",
            "http://127.0.0.1/",
            "http://192.168.0.1/admin",
            "http://[::1]/",
            "http://0x7f.1/",
            "http://example.org:abc/",
            "http://-bad.example.org/",
            "http://exa mple.org/",
            "http://example..org/",
        ] {
            bad(raw);
        }
    }

    #[test]
    fn rejects_unsafe_paths() {
        for raw in [
            "http://example.org/../etc/passwd",
            "http://example.org/a/./b.html",
            "http://example.org/a//b.html",
            "http://example.org/a\\b.html",
            "http://example.org/*",
            "http://example.org/a b.html",
            "http://example.org/docs/",
            "http://example.org/caf\u{e9}.html",
        ] {
            bad(raw);
        }
    }

    #[test]
    fn rejects_overlong_input() {
        bad(&format!("http://example.org/{}", "a".repeat(2100)));
        bad(&format!("http://{}.org/", "a".repeat(260)));
        bad(&format!("http://{}.example.org/", "a".repeat(64)));
    }

    #[test]
    fn encoding_leaves_only_unreserved_characters() {
        assert_eq!(
            encode_component("y2k.gov/a b&c=d?x#y"),
            "y2k.gov%2Fa%20b%26c%3Dd%3Fx%23y"
        );
        assert_eq!(encode_component("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn parses_cdx_answers() {
        let body = br#"[["original","timestamp","digest"],["http://x/","20000301091452","ABC"]]"#;
        let rows = parse_cdx(body).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].digest, "ABC");
        assert_eq!(parse_cdx(b"").unwrap(), vec![]);
        assert_eq!(parse_cdx(b"[]").unwrap(), vec![]);
        assert!(parse_cdx(b"<html>blocked</html>").is_err());
        assert!(parse_cdx(br#"[["h"],["only-one"]]"#).is_err());
    }

    #[test]
    fn redirects_may_not_leave_the_archive_host() {
        let c = ArchiveClient::new("https://web.archive.org", Duration::ZERO).unwrap();
        let cur = "https://web.archive.org/web/1/x";
        assert_eq!(
            c.same_host(cur, "/web/2/y").unwrap(),
            "https://web.archive.org/web/2/y"
        );
        assert_eq!(
            c.same_host(cur, "http://web.archive.org/web/2/y").unwrap(),
            "https://web.archive.org/web/2/y",
            "the scheme is rebuilt from our own base"
        );
        assert!(c.same_host(cur, "https://evil.example/web/2/y").is_err());
        assert!(c.same_host(cur, "//evil.example/x").is_err());
        assert!(c
            .same_host(cur, "https://web.archive.org.evil.example/x")
            .is_err());
        assert!(c.same_host(cur, "ftp-ish").is_err());
    }
}
