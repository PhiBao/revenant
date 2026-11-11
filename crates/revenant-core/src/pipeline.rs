//! The resurrection pipeline: one page, one target time, one ZIP and one certificate.
//!
//! See requirement R5 in `.kiro/specs/revenant-core-engine/requirements.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::autopsy::{summarize, LedgerRow};
use crate::fidelity::{classify, Capture, Verdict};
use crate::index::parse_index;
use crate::package::{build_zip, sha256_hex, Entry, PackageError};
use crate::rewrite::{rewrite_html, Context, RewriteError};
use crate::time::format_cdx_timestamp;

pub const MAX_ASSETS: usize = 500;
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
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
}

/// A source backed by a fixture directory: `captures.tsv`, optional `blobs.tsv`, and `blobs/`.
pub struct SeededSource {
    name: String,
    index: crate::index::CaptureIndex,
    bodies: BTreeMap<(String, String), Vec<u8>>,
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
        })
    }

    /// Load a fixture directory. Blobs are checked against the SHA-256 in their file name.
    pub fn from_dir(name: &str, dir: &Path) -> Result<Self, String> {
        let captures = fs::read_to_string(dir.join("captures.tsv")).map_err(|e| e.to_string())?;
        let mut bodies = BTreeMap::new();
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
                bodies.insert((c[0].to_string(), c[1].to_string()), bytes);
            }
        }
        Self::new(name, &captures, bodies)
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
    pub verdict: &'static str,
    pub included: bool,
    pub capture_time: Option<String>,
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
    pub files: Vec<FileRecord>,
    pub links_not_restored: Vec<String>,
    pub scripts_removed: usize,
    pub archive_urls_unwrapped: usize,
    pub archive_host_text_mentions: usize,
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

pub fn resurrect(
    source: &dyn SnapshotSource,
    req: &Request,
) -> Result<Resurrection, PipelineError> {
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
    let page_sel = classify(req.target, &page_caps);
    let usable = match page_sel.verdict {
        Verdict::Missing => false,
        Verdict::Anachronism => req.include_anachronisms,
        _ => true,
    };
    if !usable {
        return Err(PipelineError::NoCapture {
            verdict: page_sel.verdict,
            detail: format!("{page_label} has no usable capture at the target time"),
        });
    }
    let page_cap = page_sel.capture.clone().expect("usable has a capture");
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
    if discovery.assets.len() > MAX_ASSETS {
        return Err(PipelineError::TooManyAssets {
            found: discovery.assets.len(),
            cap: MAX_ASSETS,
        });
    }

    // 3. Classify and fetch every asset.
    let mut total_bytes = page_body.len();
    let mut records: Vec<FileRecord> = Vec::new();
    let mut ledger: Vec<LedgerRow> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut restored: BTreeSet<String> = BTreeSet::new();

    ledger.push(LedgerRow {
        label: page_path.clone(),
        verdict: page_sel.verdict,
    });
    restored.insert(page_path.clone());

    for asset in &discovery.assets {
        let label = format!("{}/{}", req.site, asset);
        let (verdict, chosen, note): (Verdict, Option<Capture>, Option<String>) =
            match source.lookup(&label) {
                Lookup::Unknown(why) => (Verdict::Unknown, None, Some(why)),
                Lookup::Captures(caps) => {
                    let sel = classify(req.target, &caps);
                    (sel.verdict, sel.capture, None)
                }
            };
        ledger.push(LedgerRow {
            label: asset.clone(),
            verdict,
        });

        let wanted = match verdict {
            Verdict::Missing | Verdict::Unknown => false,
            Verdict::Anachronism => req.include_anachronisms,
            _ => true,
        };
        let mut record = FileRecord {
            path: asset.clone(),
            verdict: verdict.as_str(),
            included: false,
            capture_time: chosen.as_ref().map(|c| format_cdx_timestamp(c.ts)),
            delta_days: chosen.as_ref().map(|c| (c.ts - req.target) / 86_400),
            archive_digest: chosen.as_ref().map(|c| c.digest.clone()),
            sha256: None,
            bytes: None,
            note,
        };
        if wanted {
            let cap = chosen.expect("wanted has a capture");
            match source.body(&label, &cap) {
                Ok(bytes) => {
                    total_bytes += bytes.len();
                    if total_bytes > MAX_BYTES {
                        return Err(PipelineError::TooLarge {
                            cap_bytes: MAX_BYTES,
                        });
                    }
                    record.included = true;
                    record.sha256 = Some(sha256_hex(&bytes));
                    record.bytes = Some(bytes.len());
                    restored.insert(asset.clone());
                    entries.push(Entry {
                        path: asset.clone(),
                        bytes,
                    });
                }
                Err(e) => {
                    record.verdict = Verdict::Unknown.as_str();
                    record.note = Some(e.0);
                    if let Some(row) = ledger.last_mut() {
                        row.verdict = Verdict::Unknown;
                    }
                }
            }
        } else if verdict == Verdict::Anachronism {
            record.note = Some("left out: first captured after the target time".into());
        } else if verdict == Verdict::Missing {
            record.note =
                Some("no usable capture (archived only as an error page, or not at all)".into());
        }
        records.push(record);
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
        verdict: page_sel.verdict.as_str(),
        included: true,
        capture_time: Some(format_cdx_timestamp(page_cap.ts)),
        delta_days: Some((page_cap.ts - req.target) / 86_400),
        archive_digest: Some(page_cap.digest.clone()),
        sha256: Some(sha256_hex(&rewritten.bytes)),
        bytes: Some(rewritten.bytes.len()),
        note: Some("sha256 is of the rewritten page, not the archived original".into()),
    };
    entries.push(Entry {
        path: page_path.clone(),
        bytes: rewritten.bytes.clone(),
    });
    let mut files = vec![page_record];
    files.extend(records);

    let vitals = {
        let a = summarize(ledger);
        Vitals {
            completeness_percent: a.completeness.map(round1),
            fidelity_percent: a.fidelity.map(round1),
            unknown_files: a.unknown,
        }
    };

    let certificate = Certificate {
        schema: "revenant.certificate/1",
        site: req.site.clone(),
        page: page_path,
        target_time: format_cdx_timestamp(req.target),
        mode: source.mode(),
        archive: source.archive_name().to_string(),
        policy: if req.include_anachronisms {
            "include-anachronisms"
        } else {
            "strict"
        },
        vitals,
        files,
        links_not_restored: rewritten.links.clone(),
        scripts_removed: rewritten.scripts_removed,
        archive_urls_unwrapped: rewritten.archive_urls_unwrapped,
        archive_host_text_mentions: mentions,
        notes: vec![
            "Archive digests are the archive's claim. File identity here is the SHA-256 of the bytes.",
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
