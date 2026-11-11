//! End-to-end restore of `briefings.html` as of 2000-03-02: classify every referenced file,
//! take each file's chosen capture, package the result, and pin the ZIP hash.
//!
//! Inputs: `fixtures/snapshots/y2k-briefings/{captures.tsv,blobs.tsv,blobs/}`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use revenant_core::fidelity::{classify, Verdict};
use revenant_core::index::parse_index;
use revenant_core::package::{build_zip, sha256_hex, Entry};
use revenant_core::time::parse_cdx_timestamp;

const INDEX: &str = include_str!("../../../fixtures/snapshots/y2k-briefings/captures.tsv");
const BLOBS: &str = include_str!("../../../fixtures/snapshots/y2k-briefings/blobs.tsv");

/// SHA-256 of the restored ZIP. Pinned: the same inputs must always give this hash.
const PINNED_ZIP_SHA256: &str = "2fd180e1c20800e8b82c849092cb4a634335de6699137b64ce645121173c08f2";

const REFERENCED: [&str; 12] = [
    "y2k.gov/briefings.html",
    "y2k.gov/images/1.gif",
    "y2k.gov/images/2_briefings.gif",
    "y2k.gov/images/3.gif",
    "y2k.gov/images/4.gif",
    "y2k.gov/images/5.gif",
    "y2k.gov/images/6.gif",
    "y2k.gov/images/7.gif",
    "y2k.gov/images/8.gif",
    "y2k.gov/images/9.gif",
    "y2k.gov/images/briefings_title1.gif",
    "y2k.gov/images/briefings_title2.gif",
];

fn blob_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/snapshots/y2k-briefings/blobs")
}

/// (label, ts) -> sha256 from the blob manifest.
fn blob_index() -> HashMap<(String, String), String> {
    BLOBS
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            ((c[0].to_string(), c[1].to_string()), c[3].to_string())
        })
        .collect()
}

fn restore(target_cdx: &str) -> Vec<Entry> {
    let target = parse_cdx_timestamp(target_cdx).expect("valid target");
    let idx = parse_index(INDEX).expect("index parses");
    let blobs = blob_index();
    let mut entries = Vec::new();
    for label in REFERENCED {
        let s = classify(target, idx.captures(label));
        if s.verdict == Verdict::Missing {
            continue;
        }
        let chosen = s.capture.expect("non-missing has a capture");
        let ts_cdx = cdx_string(chosen.ts);
        let sha = blobs
            .get(&(label.to_string(), ts_cdx.clone()))
            .unwrap_or_else(|| panic!("no blob for {label} @{ts_cdx}"));
        let bytes = fs::read(blob_dir().join(sha)).expect("blob exists");
        assert_eq!(
            sha256_hex(&bytes),
            *sha,
            "blob content does not match its name"
        );
        let path = label.trim_start_matches("y2k.gov/").to_string();
        entries.push(Entry { path, bytes });
    }
    entries
}

/// Convert Unix seconds back to a CDX timestamp, for matching against the blob manifest.
fn cdx_string(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}{:02}{:02}{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

#[test]
fn restore_at_march_2_contains_the_expected_files() {
    let entries = restore("20000302000000");
    let mut paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "briefings.html",
            "images/1.gif",
            "images/3.gif",
            "images/4.gif",
            "images/5.gif",
            "images/6.gif",
            "images/7.gif",
        ]
    );
}

#[test]
fn restore_zip_is_deterministic_regardless_of_input_order() {
    let mut a = restore("20000302000000");
    let zip_a = build_zip(&a).expect("packages");
    a.reverse();
    let zip_b = build_zip(&a).expect("packages");
    assert_eq!(zip_a, zip_b);
}

#[test]
fn restore_zip_hash_is_pinned() {
    let zip = build_zip(&restore("20000302000000")).expect("packages");
    assert_eq!(sha256_hex(&zip), PINNED_ZIP_SHA256);
}

#[test]
fn cdx_string_round_trips() {
    for s in ["20000301091452", "19991231235959", "20000229120000"] {
        let unix = parse_cdx_timestamp(s).expect("valid");
        assert_eq!(cdx_string(unix), s);
    }
}

mod rewritten {
    use super::*;
    use revenant_core::rewrite::{rewrite_html, Context};
    use std::collections::BTreeSet;

    fn page() -> (Vec<Entry>, Vec<u8>) {
        let entries = restore("20000302000000");
        let set: BTreeSet<String> = entries.iter().map(|e| e.path.clone()).collect();
        let html = entries
            .iter()
            .find(|e| e.path == "briefings.html")
            .expect("page present")
            .bytes
            .clone();
        let ctx = Context {
            page_path: "briefings.html",
            restored: &set,
            site_hosts: &["y2k.gov"],
        };
        let out = rewrite_html(&html, &ctx).expect("rewrites");
        (entries, out.bytes)
    }

    #[test]
    fn rewritten_page_has_no_archive_host() {
        let (_, bytes) = page();
        let text: String = bytes.iter().map(|&b| b as char).collect();
        assert!(!text.contains("web.archive.org"));
    }

    #[test]
    fn rewritten_page_flags_the_pages_that_were_not_restored() {
        let (_, bytes) = page();
        let text: String = bytes.iter().map(|&b| b as char).collect();
        // The briefings page links to dozens of docs/*.html pages that are not in this restore.
        assert!(text.matches("data-revenant=\"not-restored\"").count() > 10);
    }

    #[test]
    fn restored_images_stay_linked() {
        let (_, bytes) = page();
        let text: String = bytes.iter().map(|&b| b as char).collect();
        for n in [1, 3, 4, 5, 6, 7] {
            assert!(text.contains(&format!("images/{n}.gif")), "images/{n}.gif");
        }
    }
}
