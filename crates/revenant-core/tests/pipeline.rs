//! Pipeline tests (R5). The seeded cases run on the real y2k briefings fixture. The mock source
//! simulates archive failures that a fixture cannot show.

use std::collections::BTreeMap;
use std::path::PathBuf;

use revenant_core::fidelity::{Capture, Verdict};
use revenant_core::pipeline::{
    resurrect, resurrect_with, Limits, Lookup, Mode, PipelineError, Request, SeededSource,
    SnapshotSource, SourceError, CERTIFICATE_PATH,
};
use revenant_core::time::parse_cdx_timestamp;

fn fixture() -> SeededSource {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/snapshots/y2k-briefings");
    SeededSource::from_dir("fixture:y2k-briefings", &dir).expect("fixture loads")
}

fn request(target: &str, include_anachronisms: bool) -> Request {
    Request {
        site: "y2k.gov".into(),
        page: "briefings.html".into(),
        target: parse_cdx_timestamp(target).expect("valid"),
        include_anachronisms,
    }
}

/// Names inside a stored (uncompressed) ZIP, read from its central directory.
fn zip_names(zip: &[u8]) -> Vec<String> {
    let eocd = zip
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .expect("end of central directory");
    let count = u16::from_le_bytes([zip[eocd + 10], zip[eocd + 11]]) as usize;
    let mut pos = u32::from_le_bytes([
        zip[eocd + 16],
        zip[eocd + 17],
        zip[eocd + 18],
        zip[eocd + 19],
    ]) as usize;
    let mut names = Vec::new();
    for _ in 0..count {
        assert_eq!(&zip[pos..pos + 4], b"PK\x01\x02");
        let name_len = u16::from_le_bytes([zip[pos + 28], zip[pos + 29]]) as usize;
        let extra = u16::from_le_bytes([zip[pos + 30], zip[pos + 31]]) as usize;
        let comment = u16::from_le_bytes([zip[pos + 32], zip[pos + 33]]) as usize;
        names.push(String::from_utf8(zip[pos + 46..pos + 46 + name_len].to_vec()).unwrap());
        pos += 46 + name_len + extra + comment;
    }
    names.sort();
    names
}

#[test]
fn seeded_restore_includes_anachronisms_when_asked() {
    let r = resurrect(&fixture(), &request("20000302000000", true)).expect("restores");
    assert_eq!(
        zip_names(&r.zip),
        [
            CERTIFICATE_PATH,
            "briefings.html",
            "images/1.gif",
            "images/3.gif",
            "images/4.gif",
            "images/5.gif",
            "images/6.gif",
            "images/7.gif",
        ]
    );
    let c = &r.certificate;
    assert_eq!(c.mode, Mode::Seeded);
    assert_eq!(c.policy, "include-anachronisms");
    assert_eq!(c.files[0].verdict, "UNCERTAIN");
    // Completeness 7 of 12 files; no file is Proven or Likely, so fidelity is 0.
    assert_eq!(c.vitals.completeness_percent, Some(58.3));
    assert_eq!(c.vitals.fidelity_percent, Some(0.0));
    assert_eq!(c.vitals.unknown_files, 0);
    // The six images are first archived two days after the target.
    let anachronisms: Vec<_> = c
        .files
        .iter()
        .filter(|f| f.verdict == "ANACHRONISM")
        .collect();
    assert_eq!(anachronisms.len(), 6);
    assert!(anachronisms
        .iter()
        .all(|f| f.included && f.delta_days == Some(2)));
}

#[test]
fn strict_policy_keeps_anachronisms_out_but_lists_them() {
    let r = resurrect(&fixture(), &request("20000302000000", false)).expect("restores");
    assert_eq!(zip_names(&r.zip), [CERTIFICATE_PATH, "briefings.html"]);
    let left_out: Vec<_> = r
        .certificate
        .files
        .iter()
        .filter(|f| f.verdict == "ANACHRONISM")
        .collect();
    assert_eq!(left_out.len(), 6);
    assert!(left_out.iter().all(|f| !f.included && f.note.is_some()));
}

#[test]
fn the_same_request_gives_the_same_zip_bytes() {
    let a = resurrect(&fixture(), &request("20000302000000", true)).unwrap();
    let b = resurrect(&fixture(), &request("20000302000000", true)).unwrap();
    assert_eq!(a.zip, b.zip);
    assert_eq!(a.zip_sha256, b.zip_sha256);
}

#[test]
fn restored_page_has_no_archive_url_attributes() {
    let r = resurrect(&fixture(), &request("20000302000000", true)).unwrap();
    assert_eq!(r.certificate.archive_host_text_mentions, 0);
    assert!(r.certificate.links_not_restored.len() > 10);
}

#[test]
fn after_the_images_were_archived_they_are_likely() {
    // Target is one day after the images were archived and nothing newer exists.
    let r = resurrect(&fixture(), &request("20000305000000", true)).unwrap();
    let images: Vec<_> = r
        .certificate
        .files
        .iter()
        .filter(|f| f.path.starts_with("images/") && f.included)
        .collect();
    assert_eq!(images.len(), 6);
    assert!(images.iter().all(|f| f.verdict == "LIKELY"));
    assert!(r.certificate.vitals.fidelity_percent.unwrap() > 80.0);
}

#[test]
fn page_with_no_capture_before_target_is_refused_under_strict_policy() {
    let err = resurrect(&fixture(), &request("19990101000000", false)).unwrap_err();
    assert!(matches!(
        err,
        PipelineError::NoCapture {
            verdict: Verdict::Anachronism,
            ..
        }
    ));
}

#[test]
fn a_page_that_does_not_exist_is_refused() {
    let mut req = request("20000302000000", true);
    req.page = "nothing-here.html".into();
    assert!(matches!(
        resurrect(&fixture(), &req),
        Err(PipelineError::NoCapture {
            verdict: Verdict::Missing,
            ..
        })
    ));
}

#[test]
fn invalid_requests_are_rejected() {
    let mut req = request("20000302000000", true);
    req.site = "y2k.gov/evil".into();
    assert!(matches!(
        resurrect(&fixture(), &req),
        Err(PipelineError::InvalidRequest(_))
    ));
    let mut req = request("20000302000000", true);
    req.page = "../secret".into();
    assert!(matches!(
        resurrect(&fixture(), &req),
        Err(PipelineError::InvalidRequest(_))
    ));
}

/// A source with scripted answers, for failures a fixture cannot show.
struct Mock {
    lookups: BTreeMap<String, Lookup>,
    bodies: BTreeMap<String, Vec<u8>>,
}

impl SnapshotSource for Mock {
    fn mode(&self) -> Mode {
        Mode::Live
    }
    fn archive_name(&self) -> &str {
        "mock"
    }
    fn lookup(&self, label: &str) -> Lookup {
        self.lookups
            .get(label)
            .cloned()
            .unwrap_or(Lookup::Captures(vec![]))
    }
    fn body(&self, label: &str, _c: &Capture) -> Result<Vec<u8>, SourceError> {
        self.bodies
            .get(label)
            .cloned()
            .ok_or_else(|| SourceError(format!("no body for {label}")))
    }
}

fn cap(ts: &str, digest: &str) -> Capture {
    Capture {
        ts: parse_cdx_timestamp(ts).unwrap(),
        digest: digest.into(),
        content: None,
    }
}

fn mock_with_page(html: &str) -> Mock {
    let mut m = Mock {
        lookups: BTreeMap::new(),
        bodies: BTreeMap::new(),
    };
    m.lookups.insert(
        "s.org/p.html".into(),
        Lookup::Captures(vec![cap("20000101000000", "P")]),
    );
    m.bodies
        .insert("s.org/p.html".into(), html.as_bytes().to_vec());
    m
}

fn mock_request() -> Request {
    Request {
        site: "s.org".into(),
        page: "p.html".into(),
        target: parse_cdx_timestamp("20000601000000").unwrap(),
        include_anachronisms: false,
    }
}

#[test]
fn an_archive_failure_is_unknown_not_missing() {
    let mut m = mock_with_page(r#"<img src="a.gif"><img src="b.gif">"#);
    m.lookups
        .insert("s.org/a.gif".into(), Lookup::Unknown("HTTP 429".into()));
    // b.gif answered with no captures: that one really is missing.
    let r = resurrect(&m, &mock_request()).unwrap();
    let a = r
        .certificate
        .files
        .iter()
        .find(|f| f.path == "a.gif")
        .unwrap();
    let b = r
        .certificate
        .files
        .iter()
        .find(|f| f.path == "b.gif")
        .unwrap();
    assert_eq!(a.verdict, "UNKNOWN");
    assert_eq!(a.note.as_deref(), Some("HTTP 429"));
    assert_eq!(b.verdict, "MISSING");
    assert_eq!(r.certificate.vitals.unknown_files, 1);
    // Unknown is excluded from the denominator: 1 of 2 answered files recovered (the page).
    assert_eq!(r.certificate.vitals.completeness_percent, Some(50.0));
}

#[test]
fn an_unreachable_page_is_reported_as_unknown() {
    let mut m = mock_with_page("<p>x</p>");
    m.lookups
        .insert("s.org/p.html".into(), Lookup::Unknown("timeout".into()));
    assert_eq!(
        resurrect(&m, &mock_request()),
        Err(PipelineError::PageUnknown("timeout".into()))
    );
}

#[test]
fn a_body_that_cannot_be_fetched_becomes_unknown() {
    let mut m = mock_with_page(r#"<img src="a.gif">"#);
    m.lookups.insert(
        "s.org/a.gif".into(),
        Lookup::Captures(vec![cap("20000102000000", "A")]),
    );
    // No body stored for a.gif: the fetch fails.
    let r = resurrect(&m, &mock_request()).unwrap();
    let a = r
        .certificate
        .files
        .iter()
        .find(|f| f.path == "a.gif")
        .unwrap();
    assert_eq!(a.verdict, "UNKNOWN");
    assert!(!a.included);
}

#[test]
fn too_many_assets_is_refused_and_names_the_cap() {
    let imgs: String = (0..501)
        .map(|i| format!(r#"<img src="i{i}.gif">"#))
        .collect();
    let m = mock_with_page(&imgs);
    assert_eq!(
        resurrect(&m, &mock_request()),
        Err(PipelineError::TooManyAssets {
            found: 501,
            cap: 500
        })
    );
}

#[test]
fn an_oversize_job_is_refused() {
    let mut m = mock_with_page(r#"<img src="big.gif">"#);
    m.lookups.insert(
        "s.org/big.gif".into(),
        Lookup::Captures(vec![cap("20000102000000", "B")]),
    );
    m.bodies
        .insert("s.org/big.gif".into(), vec![0u8; 65 * 1024 * 1024]);
    assert!(matches!(
        resurrect(&m, &mock_request()),
        Err(PipelineError::TooLarge { .. })
    ));
}

#[test]
fn the_certificate_is_inside_the_zip() {
    let r = resurrect(&fixture(), &request("20000302000000", true)).unwrap();
    let json = serde_json::to_string_pretty(&r.certificate).unwrap();
    let needle = json.as_bytes();
    assert!(r.zip.windows(needle.len()).any(|w| w == needle));
}

#[test]
fn a_caller_can_lower_the_caps() {
    let imgs: String = (0..11)
        .map(|i| format!(r#"<img src="i{i}.gif">"#))
        .collect();
    let m = mock_with_page(&imgs);
    assert_eq!(
        resurrect_with(&m, &mock_request(), Limits::new(10, 1024)),
        Err(PipelineError::TooManyAssets { found: 11, cap: 10 })
    );
}

#[test]
fn a_caller_cannot_raise_the_caps() {
    assert_eq!(Limits::new(10_000, usize::MAX), Limits::default());
    let imgs: String = (0..501)
        .map(|i| format!(r#"<img src="i{i}.gif">"#))
        .collect();
    let m = mock_with_page(&imgs);
    let raised = Limits {
        max_assets: 10_000,
        max_bytes: usize::MAX,
    };
    assert_eq!(
        resurrect_with(&m, &mock_request(), raised),
        Err(PipelineError::TooManyAssets {
            found: 501,
            cap: 500
        })
    );
}
