//! The resurrection pipeline: one page, one target time, one ZIP and one certificate.
//!
//! See requirement R5 in `.kiro/specs/revenant-core-engine/requirements.md`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::autopsy::{summarize, LedgerRow};
use crate::css::rewrite_css;
use crate::fidelity::{classify, nearest, Capture, Selection, Verdict};
use crate::index::parse_index;
use crate::package::{build_zip, sha256_hex, Entry, PackageError};
use crate::rewrite::{rewrite_html, Context, RewriteError};
use crate::time::{format_cdx_timestamp, parse_http_date};

pub const MAX_ASSETS: usize = 500;
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
/// How many stylesheets deep `@import` is followed (R9.6).
pub const MAX_CSS_DEPTH: usize = 3;
pub const CERTIFICATE_PATH: &str = "REVENANT-CERTIFICATE.json";
const ARCHIVE_HOST: &str = "web.archive.org";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Mode {
    #[serde(rename = "LIVE")]
    Live,
    #[serde(rename = "SEEDED")]
    Seeded,
}

/// The answer to "what captures exist for this file?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// The archive answered. The list may be empty.
    Captures(Vec<Capture>),
    /// The archive could not be asked (error, block, timeout). This is not the same as no captures.
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError(pub String);

/// Where captures and their bodies come from. Live and seeded sources share this one interface.
pub trait SnapshotSource {
    fn mode(&self) -> Mode;
    fn archive_name(&self) -> &str;
    /// HTTP 200 captures for a file label such as `y2k.gov/images/1.gif`.
    fn lookup(&self, label: &str) -> Lookup;
    /// The bytes of one capture.
    fn body(&self, label: &str, capture: &Capture) -> Result<Vec<u8>, SourceError>;
    /// The origin server's Last-Modified time for one capture, as Unix seconds (R6.8).
    /// The default uses what the capture already carries. A live source fetches it on demand.
    fn origin_modified(&self, _label: &str, capture: &Capture) -> Result<Option<i64>, SourceError> {
        Ok(capture.origin_modified)
    }
}

/// A source backed by a fixture directory: `captures.tsv`, optional `blobs.tsv`, and `blobs/`.
pub struct SeededSource {
    name: String,
    index: crate::index::CaptureIndex,
    bodies: BTreeMap<(String, String), Vec<u8>>,
    /// Server Last-Modified times from the blob manifest, keyed like `bodies`.
    modified: BTreeMap<(String, String), i64>,
}

impl SeededSource {
    pub fn new(
        name: &str,
        captures_tsv: &str,
        bodies: BTreeMap<(String, String), Vec<u8>>,
    ) -> Result<Self, String> {
        let index = parse_index(captures_tsv).map_err(|e| format!("{e:?}"))?;
        Ok(Self {
            name: name.to_string(),
            index,
            bodies,
            modified: BTreeMap::new(),
        })
    }

    /// Load a fixture directory. Blobs are checked against the SHA-256 in their file name.
    pub fn from_dir(name: &str, dir: &Path) -> Result<Self, String> {
        let captures = fs::read_to_string(dir.join("captures.tsv")).map_err(|e| e.to_string())?;
        let mut bodies = BTreeMap::new();
        let mut modified = BTreeMap::new();
        if let Ok(manifest) = fs::read_to_string(dir.join("blobs.tsv")) {
            for line in manifest.lines().skip(1).filter(|l| !l.trim().is_empty()) {
                let c: Vec<&str> = line.split('\t').collect();
                if c.len() < 4 {
                    return Err(format!("bad blobs.tsv row: {line}"));
                }
                let bytes = fs::read(dir.join("blobs").join(c[3])).map_err(|e| e.to_string())?;
                if sha256_hex(&bytes) != c[3] {
                    return Err(format!("blob {} does not match its hash", c[3]));
                }
                let key = (c[0].to_string(), c[1].to_string());
                // The seventh column is the server's Last-Modified header, when it had one.
                if let Some(date) = c.get(6).and_then(|h| parse_http_date(h)) {
                    modified.insert(key.clone(), date);
                }
                bodies.insert(key, bytes);
            }
        }
        let mut source = Self::new(name, &captures, bodies)?;
        source.modified = modified;
        Ok(source)
    }
}

impl SnapshotSource for SeededSource {
    fn mode(&self) -> Mode {
        Mode::Seeded
    }

    fn archive_name(&self) -> &str {
        &self.name
    }

    fn lookup(&self, label: &str) -> Lookup {
        let mut caps = self.index.captures(label).to_vec();
        for cap in &mut caps {
            let key = (label.to_string(), format_cdx_timestamp(cap.ts));
            if let Some(bytes) = self.bodies.get(&key) {
                cap.content = Some(sha256_hex(bytes));
            }
            cap.origin_modified = self.modified.get(&key).copied();
        }
        Lookup::Captures(caps)
    }

    fn body(&self, label: &str, capture: &Capture) -> Result<Vec<u8>, SourceError> {
        let key = (label.to_string(), format_cdx_timestamp(capture.ts));
        self.bodies
            .get(&key)
            .cloned()
            .ok_or_else(|| SourceError(format!("no stored body for {label} @{}", key.1)))
    }
}

/// Caps for one job. A caller can lower them. They can never be raised above the defaults (R5.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_assets: usize,
    pub max_bytes: usize,
}

impl Limits {
    pub fn new(max_assets: usize, max_bytes: usize) -> Self {
        Self {
            max_assets: max_assets.min(MAX_ASSETS),
            max_bytes: max_bytes.min(MAX_BYTES),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_assets: MAX_ASSETS,
            max_bytes: MAX_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Site host, for example `y2k.gov`.
    pub site: String,
    /// Page path inside the site, for example `briefings.html`. Empty means the site root.
    pub page: String,
    /// Target time as Unix seconds (UTC).
    pub target: i64,
    /// Put anachronistic files in the ZIP (flagged in the certificate) or leave them out.
    pub include_anachronisms: bool,
    /// Restore the way archive playback does: for every file, the capture closest in time (R7).
    /// Revenant's verdicts are still reported, but they do not decide what goes in the ZIP.
    pub nearest: bool,
    /// Classify from capture dates alone: ignore every server date and never ask for one (R10).
    /// A comparison against the strict restore, not a recommendation.
    pub dates_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    InvalidRequest(String),
    /// The page has nothing that can be restored at the target time.
    NoCapture {
        verdict: Verdict,
        detail: String,
    },
    /// The archive could not be asked about the page.
    PageUnknown(String),
    Source(String),
    TooManyAssets {
        found: usize,
        cap: usize,
    },
    TooLarge {
        cap_bytes: usize,
    },
    Rewrite(String),
    Package(String),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileRecord {
    pub path: String,
    /// The stylesheet this file was found in. Absent for files the page itself names (R9).
    pub found_in: Option<String>,
    pub verdict: &'static str,
    /// What capture dates alone would have said (R6.9).
    pub verdict_from_dates_only: &'static str,
    /// Why the verdict was reached, in plain words (R6.7).
    pub basis: String,
    pub included: bool,
    pub capture_time: Option<String>,
    /// The origin server's Last-Modified for the chosen capture, as a CDX-style timestamp.
    pub origin_modified: Option<String>,
    /// Chosen capture minus target, in whole days. Negative means before the target.
    pub delta_days: Option<i64>,
    pub archive_digest: Option<String>,
    pub sha256: Option<String>,
    pub bytes: Option<usize>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Vitals {
    pub completeness_percent: Option<f64>,
    pub fidelity_percent: Option<f64>,
    pub unknown_files: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Certificate {
    pub schema: &'static str,
    pub site: String,
    pub page: String,
    pub target_time: String,
    pub mode: Mode,
    pub archive: String,
    pub policy: &'static str,
    pub vitals: Vitals,
    /// The same files scored from capture dates alone, so the effect of server dates is visible.
    pub vitals_from_dates_only: Vitals,
    pub files: Vec<FileRecord>,
    pub links_not_restored: Vec<String>,
    pub scripts_removed: usize,
    pub archive_urls_unwrapped: usize,
    pub archive_host_text_mentions: usize,
    /// Script-running CSS constructs neutralised, in the page and in stylesheets (R9.5).
    pub css_constructs_neutralised: usize,
    pub notes: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resurrection {
    /// The files inside the ZIP, in the order they were added. Used to serve previews.
    pub entries: Vec<Entry>,
    pub zip: Vec<u8>,
    pub zip_sha256: String,
    pub certificate: Certificate,
}

/// A file waiting to be judged and fetched.
struct Queued {
    path: String,
    /// 1 for a file the page names. One more for each stylesheet it was found through.
    depth: usize,
    /// The stylesheet this file was found in, when it was not found in the page.
    found_in: Option<String>,
    /// Pulled in with `@import`, so it is a stylesheet whatever its name.
    is_import: bool,
}

/// A stylesheet that is in the ZIP and still needs its final rewrite.
struct Stylesheet {
    path: String,
    original: Vec<u8>,
    entry_index: usize,
    record_index: usize,
}

/// What the pipeline concluded about one file at the target time.
struct Judged {
    /// The verdict with server evidence (R6).
    sel: Selection,
    /// The verdict capture dates alone would give.
    dates_only: Verdict,
    /// All captures, with the later one's server date filled in when it was asked for.
    caps: Vec<Capture>,
}

/// Classify one file, asking the source for a server date only when it could change the answer
/// (R6.8).
fn judge(
    source: &dyn SnapshotSource,
    label: &str,
    target: i64,
    caps: Vec<Capture>,
    use_server_dates: bool,
) -> Judged {
    let stripped: Vec<Capture> = caps
        .iter()
        .cloned()
        .map(|mut c| {
            c.origin_modified = None;
            c
        })
        .collect();
    let dates_only = classify(target, &stripped).verdict;

    if !use_server_dates {
        // R10.1: the dates-only restore sees no server date at all.
        return Judged {
            sel: classify(target, &stripped),
            dates_only,
            caps: stripped,
        };
    }
    let mut caps = caps;
    let mut sel = classify(target, &caps);
    if matches!(sel.verdict, Verdict::Anachronism | Verdict::Uncertain) {
        let later = caps
            .iter()
            .enumerate()
            .filter(|(_, c)| c.ts > target)
            .min_by_key(|(_, c)| (c.ts, c.digest.clone()))
            .map(|(i, _)| i);
        if let Some(i) = later {
            if caps[i].origin_modified.is_none() {
                // If the source cannot say, the verdict stays as the dates left it.
                if let Ok(Some(modified)) = source.origin_modified(label, &caps[i]) {
                    caps[i].origin_modified = Some(modified);
                    sel = classify(target, &caps);
                }
            }
        }
    }
    Judged {
        sel,
        dates_only,
        caps,
    }
}

/// Whether the ZIP should hold this file, and which capture to use.
fn choose<'a>(req: &Request, j: &'a Judged) -> (bool, Option<&'a Capture>) {
    if req.nearest {
        let c = nearest(req.target, &j.caps);
        return (c.is_some(), c);
    }
    let wanted = match j.sel.verdict {
        Verdict::Missing | Verdict::Unknown => false,
        Verdict::Anachronism => req.include_anachronisms,
        _ => true,
    };
    (wanted, j.sel.capture.as_ref())
}

fn modified_text(c: Option<&Capture>) -> Option<String> {
    c.and_then(|c| c.origin_modified).map(format_cdx_timestamp)
}

fn describe_choice(req: &Request, j: &Judged, used: Option<&Capture>) -> Option<String> {
    if !req.nearest {
        return j.sel.note.map(str::to_string);
    }
    let theirs = j.sel.capture.as_ref()?;
    let used = used?;
    if theirs.ts == used.ts {
        return j.sel.note.map(str::to_string);
    }
    Some(format!(
        "Playback takes the closest copy; Revenant would use the one from {}.",
        format_cdx_timestamp(theirs.ts)
    ))
}

pub fn resurrect(
    source: &dyn SnapshotSource,
    req: &Request,
) -> Result<Resurrection, PipelineError> {
    resurrect_with(source, req, Limits::default())
}

pub fn resurrect_with(
    source: &dyn SnapshotSource,
    req: &Request,
    limits: Limits,
) -> Result<Resurrection, PipelineError> {
    // Callers cannot raise the caps by constructing the struct by hand.
    let limits = Limits::new(limits.max_assets, limits.max_bytes);
    if req.site.is_empty() || req.site.contains('/') {
        return Err(PipelineError::InvalidRequest(
            "site must be a bare host".into(),
        ));
    }
    let page_path = if req.page.is_empty() {
        "index.html".to_string()
    } else {
        req.page.clone()
    };
    crate::package::sanitize_path(&page_path)
        .map_err(|r| PipelineError::InvalidRequest(format!("page path: {r:?}")))?;
    let page_label = if req.page.is_empty() {
        format!("{}/", req.site)
    } else {
        format!("{}/{}", req.site, req.page)
    };

    // 1. The page itself.
    let page_caps = match source.lookup(&page_label) {
        Lookup::Captures(c) => c,
        Lookup::Unknown(why) => return Err(PipelineError::PageUnknown(why)),
    };
    let page = judge(source, &page_label, req.target, page_caps, !req.dates_only);
    let (page_usable, page_choice) = choose(req, &page);
    if !page_usable {
        return Err(PipelineError::NoCapture {
            verdict: page.sel.verdict,
            detail: format!("{page_label} has no usable capture at the target time"),
        });
    }
    let page_cap = page_choice.expect("usable has a capture").clone();
    let page_body = source
        .body(&page_label, &page_cap)
        .map_err(|e| PipelineError::Source(e.0))?;

    // 2. Discover assets from the page itself (R5.2).
    let hosts = [req.site.as_str()];
    let empty = BTreeSet::new();
    let discovery = rewrite_html(
        &page_body,
        &Context {
            page_path: &page_path,
            restored: &empty,
            site_hosts: &hosts,
        },
    )
    .map_err(rewrite_error)?;
    if discovery.assets.len() > limits.max_assets {
        return Err(PipelineError::TooManyAssets {
            found: discovery.assets.len(),
            cap: limits.max_assets,
        });
    }

    // 3. Judge and fetch every asset, following stylesheets (R9).
    let mut total_bytes = page_body.len();
    let mut records: Vec<FileRecord> = Vec::new();
    let mut ledger: Vec<LedgerRow> = Vec::new();
    let mut ledger_dates: Vec<LedgerRow> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut restored: BTreeSet<String> = BTreeSet::new();

    ledger.push(LedgerRow {
        label: page_path.clone(),
        verdict: page.sel.verdict,
    });
    ledger_dates.push(LedgerRow {
        label: page_path.clone(),
        verdict: page.dates_only,
    });
    restored.insert(page_path.clone());

    // Every path ever queued, the page included. Its size is what the asset cap counts (R9.7), and
    // a stylesheet that imports itself is read once (R9.6).
    let mut seen: BTreeSet<String> = BTreeSet::new();
    seen.insert(page_path.clone());
    let mut queue: VecDeque<Queued> = VecDeque::new();
    for a in &discovery.assets {
        if seen.insert(a.clone()) {
            queue.push_back(Queued {
                path: a.clone(),
                depth: 1,
                found_in: None,
                is_import: false,
            });
        }
    }
    let mut stylesheets: Vec<Stylesheet> = Vec::new();

    while let Some(item) = queue.pop_front() {
        let asset = &item.path;
        let label = format!("{}/{}", req.site, asset);
        let judged = match source.lookup(&label) {
            Lookup::Unknown(why) => {
                ledger.push(LedgerRow {
                    label: asset.clone(),
                    verdict: Verdict::Unknown,
                });
                ledger_dates.push(LedgerRow {
                    label: asset.clone(),
                    verdict: Verdict::Unknown,
                });
                records.push(FileRecord {
                    path: asset.clone(),
                    found_in: item.found_in.clone(),
                    verdict: Verdict::Unknown.as_str(),
                    verdict_from_dates_only: Verdict::Unknown.as_str(),
                    basis: crate::fidelity::Basis::NotAnswered.text(),
                    included: false,
                    capture_time: None,
                    origin_modified: None,
                    delta_days: None,
                    archive_digest: None,
                    sha256: None,
                    bytes: None,
                    note: Some(why),
                });
                continue;
            }
            Lookup::Captures(caps) => judge(source, &label, req.target, caps, !req.dates_only),
        };

        let (wanted, choice) = choose(req, &judged);
        let choice = choice.cloned();
        let mut record = FileRecord {
            path: asset.clone(),
            found_in: item.found_in.clone(),
            verdict: judged.sel.verdict.as_str(),
            verdict_from_dates_only: judged.dates_only.as_str(),
            basis: judged.sel.basis.text(),
            included: false,
            capture_time: choice.as_ref().map(|c| format_cdx_timestamp(c.ts)),
            origin_modified: modified_text(choice.as_ref()),
            delta_days: choice.as_ref().map(|c| (c.ts - req.target) / 86_400),
            archive_digest: choice.as_ref().map(|c| c.digest.clone()),
            sha256: None,
            bytes: None,
            note: describe_choice(req, &judged, choice.as_ref()),
        };
        let mut row_verdict = judged.sel.verdict;
        let mut row_dates = judged.dates_only;

        if wanted {
            let cap = choice.expect("wanted has a capture");
            match source.body(&label, &cap) {
                Ok(bytes) => {
                    total_bytes += bytes.len();
                    if total_bytes > limits.max_bytes {
                        return Err(PipelineError::TooLarge {
                            cap_bytes: limits.max_bytes,
                        });
                    }
                    record.included = true;
                    record.sha256 = Some(sha256_hex(&bytes));
                    record.bytes = Some(bytes.len());
                    restored.insert(asset.clone());

                    // A stylesheet's own references are assets too (R9.1). Read it now, with
                    // nothing restored yet, to learn what it needs. It is rewritten for real
                    // once every file's fate is known.
                    let is_css = item.is_import || asset.to_ascii_lowercase().ends_with(".css");
                    if is_css {
                        let found = rewrite_css(
                            &bytes,
                            &Context {
                                page_path: asset,
                                restored: &empty,
                                site_hosts: &hosts,
                            },
                        )
                        .findings;
                        for p in found.assets {
                            let import = found.imports.contains(&p);
                            // Imports stop at a fixed depth. Plain images are always followed.
                            if import && item.depth >= MAX_CSS_DEPTH {
                                continue;
                            }
                            if seen.insert(p.clone()) {
                                if seen.len() - 1 > limits.max_assets {
                                    return Err(PipelineError::TooManyAssets {
                                        found: seen.len() - 1,
                                        cap: limits.max_assets,
                                    });
                                }
                                queue.push_back(Queued {
                                    path: p,
                                    depth: item.depth + 1,
                                    found_in: Some(asset.clone()),
                                    is_import: import,
                                });
                            }
                        }
                        stylesheets.push(Stylesheet {
                            path: asset.clone(),
                            original: bytes.clone(),
                            entry_index: entries.len(),
                            record_index: records.len(),
                        });
                    }
                    entries.push(Entry {
                        path: asset.clone(),
                        bytes,
                    });
                }
                Err(e) => {
                    record.verdict = Verdict::Unknown.as_str();
                    record.verdict_from_dates_only = Verdict::Unknown.as_str();
                    record.basis = crate::fidelity::Basis::NotAnswered.text();
                    record.note = Some(e.0);
                    row_verdict = Verdict::Unknown;
                    row_dates = Verdict::Unknown;
                }
            }
        } else if judged.sel.verdict == Verdict::Anachronism {
            record.note = Some(match judged.sel.note {
                Some(n) => format!("Left out: {n}."),
                None => "Left out: first captured after the target time.".into(),
            });
        }
        ledger.push(LedgerRow {
            label: asset.clone(),
            verdict: row_verdict,
        });
        ledger_dates.push(LedgerRow {
            label: asset.clone(),
            verdict: row_dates,
        });
        records.push(record);
    }

    // Rewrite every stylesheet now that the set of restored files is final (R9.8).
    let mut css_neutralised = 0;
    for sheet in &stylesheets {
        let out = rewrite_css(
            &sheet.original,
            &Context {
                page_path: &sheet.path,
                restored: &restored,
                site_hosts: &hosts,
            },
        );
        css_neutralised += out.findings.neutralised;
        records[sheet.record_index].sha256 = Some(sha256_hex(&out.bytes));
        records[sheet.record_index].bytes = Some(out.bytes.len());
        entries[sheet.entry_index].bytes = out.bytes;
    }

    // 4. Rewrite the page with the real restored set.
    let rewritten = rewrite_html(
        &page_body,
        &Context {
            page_path: &page_path,
            restored: &restored,
            site_hosts: &hosts,
        },
    )
    .map_err(rewrite_error)?;
    let mentions = String::from_utf8_lossy(&rewritten.bytes)
        .matches(ARCHIVE_HOST)
        .count();

    let page_record = FileRecord {
        path: page_path.clone(),
        found_in: None,
        verdict: page.sel.verdict.as_str(),
        verdict_from_dates_only: page.dates_only.as_str(),
        basis: page.sel.basis.text(),
        included: true,
        capture_time: Some(format_cdx_timestamp(page_cap.ts)),
        origin_modified: modified_text(Some(&page_cap)),
        delta_days: Some((page_cap.ts - req.target) / 86_400),
        archive_digest: Some(page_cap.digest.clone()),
        sha256: Some(sha256_hex(&rewritten.bytes)),
        bytes: Some(rewritten.bytes.len()),
        note: describe_choice(req, &page, Some(&page_cap)),
    };
    entries.push(Entry {
        path: page_path.clone(),
        bytes: rewritten.bytes.clone(),
    });
    let mut files = vec![page_record];
    files.extend(records);

    let vitals_of = |rows: Vec<LedgerRow>| {
        let a = summarize(rows);
        Vitals {
            completeness_percent: a.completeness.map(round1),
            fidelity_percent: a.fidelity.map(round1),
            unknown_files: a.unknown,
        }
    };

    let certificate = Certificate {
        schema: "revenant.certificate/2",
        site: req.site.clone(),
        page: page_path,
        target_time: format_cdx_timestamp(req.target),
        mode: source.mode(),
        archive: source.archive_name().to_string(),
        policy: if req.dates_only {
            "dates-only"
        } else if req.nearest {
            "nearest"
        } else if req.include_anachronisms {
            "include-anachronisms"
        } else {
            "strict"
        },
        vitals: vitals_of(ledger),
        vitals_from_dates_only: vitals_of(ledger_dates),
        files,
        links_not_restored: rewritten.links.clone(),
        scripts_removed: rewritten.scripts_removed,
        archive_urls_unwrapped: rewritten.archive_urls_unwrapped,
        archive_host_text_mentions: mentions,
        css_constructs_neutralised: css_neutralised + rewritten.css_neutralised,
        notes: vec![
            "Archive digests are the archive's claim. File identity here is the SHA-256 of the bytes.",
            "The SHA-256 of the page is of the rewritten page, not the archived original.",
            if req.dates_only {
                "Server dates were not used in this restore. It is a comparison made from capture dates alone."
            } else {
                "Server Last-Modified dates are the origin's claim. They are used as evidence, and ignored when they contradict the archive."
            },
            "Single-page scope: navigation links are not restored.",
        ],
    };

    let cert_bytes = serde_json::to_vec_pretty(&certificate)
        .map_err(|e| PipelineError::Package(e.to_string()))?;
    entries.push(Entry {
        path: CERTIFICATE_PATH.to_string(),
        bytes: cert_bytes,
    });

    let zip = build_zip(&entries).map_err(|e| match e {
        PackageError::InvalidPath { path, reason } => {
            PipelineError::Package(format!("{path}: {reason:?}"))
        }
        other => PipelineError::Package(format!("{other:?}")),
    })?;
    let zip_sha256 = sha256_hex(&zip);

    Ok(Resurrection {
        entries,
        zip,
        zip_sha256,
        certificate,
    })
}

fn rewrite_error(e: RewriteError) -> PipelineError {
    PipelineError::Rewrite(format!("{e:?}"))
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}
