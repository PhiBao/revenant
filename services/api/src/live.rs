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
/// The most months a timeline shows (T1.2). The archive began in 1996, so 600 months reach 2046.
pub const MAX_TIMELINE_MONTHS: usize = 600;
/// How many captures the first index answer asks for (L6.1).
pub const CLOSEST_ROWS: usize = 6;
pub const MAX_LIVE_ASSETS: usize = 80;
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
    // No real top-level domain starts with a digit. This rejects IPv4 in every spelling
    // (1.2.3.4, 127.1, 0x7f.0x1, 0177.0.0.1) without having to parse each of them (S2).
    if labels
        .last()
        .is_some_and(|l| l.bytes().next().is_some_and(|b| b.is_ascii_digit()))
    {
        return Err(UrlError(
            "IP addresses are not supported. Use the site's name.",
        ));
    }
    Ok(())
}

impl LiveTarget {
    /// The label the pipeline and the index use for this page: `y2k.gov/briefings.html`, or
    /// `y2k.gov/` for a site root.
    pub fn label(&self) -> String {
        if self.page.is_empty() {
            format!("{}/", self.site)
        } else {
            format!("{}/{}", self.site, self.page)
        }
    }
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
        self.get_with_origin_date(path_and_query, limit)
            .map(|f| f.bytes)
    }

    /// Like `get`, and also returns the origin server's Last-Modified header, which the archive
    /// preserves as `x-archive-orig-last-modified` (E2.1).
    fn get_with_origin_date(
        &self,
        path_and_query: &str,
        limit: u64,
    ) -> Result<Fetched, Unavailable> {
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
                    let modified = resp
                        .headers()
                        .get("x-archive-orig-last-modified")
                        .and_then(|v| v.to_str().ok())
                        .and_then(revenant_core::time::parse_http_date);
                    let bytes = resp
                        .body_mut()
                        .with_config()
                        .limit(limit)
                        .read_to_vec()
                        .map_err(|e| Unavailable(format!("could not read the response: {e}")))?;
                    return Ok(Fetched { bytes, modified });
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

    /// One question: the first capture in each calendar month (T1.2). Asks for one row more than the
    /// cap, so a full answer means there are more months than are shown.
    pub fn timeline(&self, key: &str) -> Result<Vec<CdxRow>, Unavailable> {
        let q = format!(
            "/cdx/search/cdx?url={}&output=json&fl=original,timestamp,digest&filter=statuscode:200&collapse=timestamp:6&limit={}",
            encode_component(key),
            MAX_TIMELINE_MONTHS + 1
        );
        let body = self.get(&q, 4 * 1024 * 1024)?;
        parse_cdx(&body)
    }

    fn replay(&self, timestamp: &str, original: &str) -> Result<Fetched, Unavailable> {
        // The original comes from the archive's own index, never from the visitor.
        self.get_with_origin_date(&format!("/web/{timestamp}id_/{original}"), MAX_BODY_BYTES)
    }
}

/// A replayed body and the origin server's own Last-Modified time, when the archive kept it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub bytes: Vec<u8>,
    pub modified: Option<i64>,
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

type BodyCache = Mutex<HashMap<(String, i64), Result<Fetched, SourceError>>>;

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
        let target = revenant_core::time::format_cdx_timestamp(self.target_ts);
        let after_start = revenant_core::time::format_cdx_timestamp(self.target_ts + 1);

        // One question, nearest first (L6.1).
        let window = format!("sort=closest&closest={target}");
        let mut rows = match self.client.cdx(label, &window, &CLOSEST_ROWS.to_string()) {
            Ok(r) => r,
            Err(e) => return Lookup::Unknown(e.0),
        };

        // The nearest rows can all fall on one side while a capture exists on the other. Ask for
        // the missing side only when the answer was full, since a short answer is everything the
        // index has (L6.2, L6.3).
        let (need_before, need_after) = follow_up_needed(&rows, self.target_ts);
        if need_before {
            match self.client.cdx(label, &format!("to={target}"), "-1") {
                Ok(more) => rows.extend(more),
                Err(e) => return Lookup::Unknown(e.0),
            }
        }
        if need_after {
            match self.client.cdx(label, &format!("from={after_start}"), "1") {
                Ok(more) => rows.extend(more),
                Err(e) => return Lookup::Unknown(e.0),
            }
        }

        let mut caps: Vec<Capture> = Vec::new();
        let mut originals = self.originals.lock().expect("originals lock");
        for row in rows {
            let Some(ts) = parse_cdx_timestamp(&row.timestamp) else {
                continue;
            };
            if caps.iter().any(|c| c.ts == ts) {
                continue; // L6.5
            }
            originals.insert(
                (label.to_string(), ts),
                (row.timestamp.clone(), row.original.clone()),
            );
            caps.push(Capture::new(ts, row.digest));
        }
        Lookup::Captures(caps)
    }
}

/// Whether the nearest-first answer is missing a side that may exist (L6.2).
///
/// Returns `(ask_for_an_earlier_one, ask_for_a_later_one)`. A side is only worth asking about
/// when the answer was full: fewer rows than requested means the index told us everything.
pub fn follow_up_needed(rows: &[CdxRow], target: i64) -> (bool, bool) {
    if rows.len() < CLOSEST_ROWS {
        return (false, false);
    }
    let times: Vec<i64> = rows
        .iter()
        .filter_map(|r| parse_cdx_timestamp(&r.timestamp))
        .collect();
    let has_before = times.iter().any(|t| *t <= target);
    let has_after = times.iter().any(|t| *t > target);
    (!has_before, !has_after)
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
        self.fetch(label, capture).map(|f| f.bytes)
    }

    /// Fetches the capture, which also yields the server date. The result is cached, so asking
    /// for the date and then using the body costs one archive request (E2.2).
    fn origin_modified(&self, label: &str, capture: &Capture) -> Result<Option<i64>, SourceError> {
        self.fetch(label, capture).map(|f| f.modified)
    }
}

impl LiveSource {
    fn fetch(&self, label: &str, capture: &Capture) -> Result<Fetched, SourceError> {
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
        // Real top-level domains that contain digits later are fine.
        assert_eq!(ok("http://example.xn--p1ai/").site, "example.xn--p1ai");
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
            "http://0x7f.0x1/",
            "http://0177.0.0.1/",
            "http://127.1/",
            "http://017700000001/",
            "http://1.2.3.4/",
            "http://example.1com/",
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

    fn row(ts: &str) -> CdxRow {
        CdxRow {
            original: "http://x/".into(),
            timestamp: ts.into(),
            digest: "D".into(),
        }
    }

    #[test]
    fn a_short_answer_is_everything_the_index_has() {
        let t = parse_cdx_timestamp("20000302235959").unwrap();
        // Five rows, all before the target: asked for six, so nothing more exists.
        let rows: Vec<CdxRow> = (1..=5).map(|d| row(&format!("2000030{d}000000"))).collect();
        assert_eq!(follow_up_needed(&rows, t), (false, false));
        assert_eq!(follow_up_needed(&[], t), (false, false));
    }

    #[test]
    fn a_full_answer_on_one_side_asks_for_the_other() {
        let t = parse_cdx_timestamp("20000302235959").unwrap();
        let after: Vec<CdxRow> = (0..6).map(|h| row(&format!("200003030{h}0000"))).collect();
        assert_eq!(
            follow_up_needed(&after, t),
            (true, false),
            "all after: ask for an earlier one"
        );
        let before: Vec<CdxRow> = (0..6).map(|h| row(&format!("200003020{h}0000"))).collect();
        assert_eq!(
            follow_up_needed(&before, t),
            (false, true),
            "all before: ask for a later one"
        );
    }

    #[test]
    fn a_full_answer_with_both_sides_needs_nothing() {
        let t = parse_cdx_timestamp("20000302235959").unwrap();
        let mut rows: Vec<CdxRow> = (0..5).map(|h| row(&format!("200003020{h}0000"))).collect();
        rows.push(row("20000304041110"));
        assert_eq!(follow_up_needed(&rows, t), (false, false));
    }

    #[test]
    fn unparseable_rows_do_not_count_as_a_side() {
        let t = parse_cdx_timestamp("20000302235959").unwrap();
        let rows: Vec<CdxRow> = (0..6).map(|_| row("garbage")).collect();
        assert_eq!(follow_up_needed(&rows, t), (true, true));
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
