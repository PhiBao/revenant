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
        nearest: false,
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

const IMAGES: [&str; 6] = [
    "images/1.gif",
    "images/3.gif",
    "images/4.gif",
    "images/5.gif",
    "images/6.gif",
    "images/7.gif",
];

fn record<'a>(
    c: &'a revenant_core::pipeline::Certificate,
    path: &str,
) -> &'a revenant_core::pipeline::FileRecord {
    c.files
        .iter()
        .find(|f| f.path == path)
        .expect("file in certificate")
}

/// The server dates (R6) turn the six images from anachronisms into attested files.
/// At midnight the page is still uncertain: it was replaced at 01:06, and nothing says
/// whether the old copy or the new one was live at 00:00.
#[test]
fn at_midnight_the_images_are_attested_and_the_page_is_uncertain() {
    let r = resurrect(&fixture(), &request("20000302000000", false)).expect("restores");
    let c = &r.certificate;
    assert_eq!(c.mode, Mode::Seeded);
    assert_eq!(c.policy, "strict");
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
    assert_eq!(c.files[0].verdict, "UNCERTAIN");
    assert_eq!(c.files[0].capture_time.as_deref(), Some("20000301091452"));
    for img in IMAGES {
        let f = record(c, img);
        assert_eq!(f.verdict, "ATTESTED", "{img}");
        assert_eq!(f.verdict_from_dates_only, "ANACHRONISM", "{img}");
        assert!(f.included, "{img}");
        assert_eq!(
            f.origin_modified.as_deref().map(|d| &d[..8]),
            Some("19991227"),
            "{img}"
        );
        assert!(f.basis.contains("1999-12-27"), "{}", f.basis);
        assert_eq!(f.delta_days, Some(2), "{img} was captured two days later");
    }
    // 7 of 12 files recovered. Six of those seven are attested; the page is not.
    assert_eq!(c.vitals.completeness_percent, Some(58.3));
    assert_eq!(c.vitals.fidelity_percent, Some(85.7));
    // From capture dates alone nothing could be trusted: the figure the v1 model reported.
    assert_eq!(c.vitals_from_dates_only.completeness_percent, Some(58.3));
    assert_eq!(c.vitals_from_dates_only.fidelity_percent, Some(0.0));
}

/// By the end of 2000-03-02 the page had been replaced (01:06). The newer copy is the true one,
/// and the latest capture before the day is stale. This is the finding that started R6.
#[test]
fn at_the_end_of_the_day_the_replaced_page_is_attested_and_the_older_copy_is_not_used() {
    let r = resurrect(&fixture(), &request("20000302235959", false)).expect("restores");
    let c = &r.certificate;
    let page = &c.files[0];
    assert_eq!(page.verdict, "ATTESTED");
    assert_eq!(page.verdict_from_dates_only, "UNCERTAIN");
    assert_eq!(
        page.capture_time.as_deref(),
        Some("20000304041110"),
        "copy B, not copy A"
    );
    assert!(page.basis.contains("2000-03-02"), "{}", page.basis);
    assert_eq!(c.vitals.fidelity_percent, Some(100.0));
    assert_eq!(c.vitals_from_dates_only.fidelity_percent, Some(0.0));
}

/// A file whose only copy the server shows was made after the date is a confirmed anachronism,
/// and the strict policy leaves it out while the permissive one keeps it (R6.4).
#[test]
fn a_confirmed_anachronism_is_left_out_under_strict_and_kept_when_asked() {
    // The mock target is 2000-06-01. Both images were first captured on 2000-07-02.
    let mut m = mock_with_page(r#"<img src="new.gif"><img src="old.gif">"#);

    // new.gif: the server says it was made on 2000-07-01, after the target.
    let mut new_gif = cap("20000702000000", "N");
    new_gif.origin_modified = Some(parse_cdx_timestamp("20000701000000").unwrap());
    m.lookups
        .insert("s.org/new.gif".into(), Lookup::Captures(vec![new_gif]));
    m.bodies.insert("s.org/new.gif".into(), b"GIF8new".to_vec());

    // old.gif: the server says it last changed in 1999, long before the target.
    let mut old_gif = cap("20000702000000", "O");
    old_gif.origin_modified = Some(parse_cdx_timestamp("19990101000000").unwrap());
    m.lookups
        .insert("s.org/old.gif".into(), Lookup::Captures(vec![old_gif]));
    m.bodies.insert("s.org/old.gif".into(), b"GIF8old".to_vec());

    let strict = resurrect(&m, &mock_request()).unwrap();
    let new = record(&strict.certificate, "new.gif");
    assert_eq!(new.verdict, "ANACHRONISM");
    assert!(!new.included);
    assert!(new.note.as_deref().unwrap().contains("made after the date"));
    assert!(new.note.as_deref().unwrap().starts_with("Left out: "));
    let old = record(&strict.certificate, "old.gif");
    assert_eq!(old.verdict, "ATTESTED");
    assert!(old.included);

    let mut permissive = mock_request();
    permissive.include_anachronisms = true;
    let kept = resurrect(&m, &permissive).unwrap();
    assert!(record(&kept.certificate, "new.gif").included);
}

#[test]
fn the_source_is_asked_for_a_server_date_only_when_it_could_change_the_answer() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Counting {
        inner: Mock,
        asked: AtomicUsize,
    }
    impl SnapshotSource for Counting {
        fn mode(&self) -> Mode {
            Mode::Live
        }
        fn archive_name(&self) -> &str {
            "counting"
        }
        fn lookup(&self, label: &str) -> Lookup {
            self.inner.lookup(label)
        }
        fn body(&self, label: &str, c: &Capture) -> Result<Vec<u8>, SourceError> {
            self.inner.body(label, c)
        }
        fn origin_modified(&self, _l: &str, _c: &Capture) -> Result<Option<i64>, SourceError> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
    }
    let mut m =
        mock_with_page(r#"<img src="proven.gif"><img src="later.gif"><img src="gone.gif">"#);
    // Proven (same content both sides): no question needed.
    m.lookups.insert(
        "s.org/proven.gif".into(),
        Lookup::Captures(vec![cap("20000101000000", "X"), cap("20000901000000", "X")]),
    );
    // Only a later copy: the server date could matter, so it is asked.
    m.lookups.insert(
        "s.org/later.gif".into(),
        Lookup::Captures(vec![cap("20000901000000", "L")]),
    );
    // Missing: nothing to ask about.
    let counting = Counting {
        inner: m,
        asked: AtomicUsize::new(0),
    };
    resurrect(&counting, &mock_request()).unwrap();
    // The page is Likely (one capture before). Only later.gif is asked about.
    assert_eq!(counting.asked.load(Ordering::SeqCst), 1);
}

/// R7: playback-style restore takes the capture closest in time and reports Revenant's verdicts.
#[test]
fn the_nearest_restore_uses_the_closest_capture_and_still_reports_revenants_verdict() {
    let mut m = mock_with_page(r#"<img src="a.gif">"#);
    // Target is 2000-06-01. Copy X is 40 days before; copy Y is 5 days after and differs.
    m.lookups.insert(
        "s.org/a.gif".into(),
        Lookup::Captures(vec![cap("20000422000000", "X"), cap("20000606000000", "Y")]),
    );
    m.bodies.insert("s.org/a.gif".into(), b"GIF8bytes".to_vec());
    // Revenant keeps the earlier copy and calls it uncertain; playback takes the closer, later one.
    let strict = resurrect(&m, &mock_request()).unwrap();
    let a = record(&strict.certificate, "a.gif");
    assert_eq!(
        (a.verdict, a.capture_time.as_deref()),
        ("UNCERTAIN", Some("20000422000000"))
    );

    let mut nearest_req = mock_request();
    nearest_req.nearest = true;
    let near = resurrect(&m, &nearest_req).unwrap();
    assert_eq!(near.certificate.policy, "nearest");
    let a = record(&near.certificate, "a.gif");
    assert_eq!(
        a.capture_time.as_deref(),
        Some("20000606000000"),
        "the closer capture"
    );
    assert_eq!(
        a.verdict, "UNCERTAIN",
        "Revenant's verdict is still reported"
    );
    assert!(a
        .note
        .as_deref()
        .unwrap()
        .contains("Revenant would use the one from 20000422000000"));
}

/// R7.3: when the two methods pick the same files, every file but the certificate matches.
#[test]
fn when_the_methods_agree_every_restored_file_is_identical() {
    let strict = resurrect(&fixture(), &request("20000302235959", false)).unwrap();
    let mut req = request("20000302235959", false);
    req.nearest = true;
    let near = resurrect(&fixture(), &req).unwrap();
    let files = |r: &revenant_core::pipeline::Resurrection| {
        r.certificate
            .files
            .iter()
            .map(|f| (f.path.clone(), f.included, f.sha256.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(files(&strict), files(&near));
    assert_ne!(
        strict.zip_sha256, near.zip_sha256,
        "the certificates name different policies"
    );
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
    Capture::new(parse_cdx_timestamp(ts).unwrap(), digest)
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
        nearest: false,
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

// ---------------------------------------------------------------- R9: stylesheets

/// A mock site where every named file has one capture before the mock target (2000-06-01).
fn site(files: &[(&str, &str)]) -> Mock {
    let page = files
        .iter()
        .find(|(p, _)| *p == "p.html")
        .map(|(_, b)| *b)
        .expect("p.html in the site");
    let mut m = mock_with_page(page);
    for (path, body) in files.iter().filter(|(p, _)| *p != "p.html") {
        let label = format!("s.org/{path}");
        m.lookups.insert(
            label.clone(),
            Lookup::Captures(vec![cap("20000101000000", path)]),
        );
        m.bodies.insert(label, body.as_bytes().to_vec());
    }
    m
}

fn included(c: &revenant_core::pipeline::Certificate) -> Vec<&str> {
    let mut v: Vec<&str> = c
        .files
        .iter()
        .filter(|f| f.included)
        .map(|f| f.path.as_str())
        .collect();
    v.sort();
    v
}

#[test]
fn a_stylesheets_own_references_are_restored_and_say_where_they_were_found() {
    let m = site(&[
        ("p.html", r#"<link rel="stylesheet" href="css/site.css">"#),
        (
            "css/site.css",
            "body{background:url(../img/bg.gif)} @import 'more.css';",
        ),
        ("css/more.css", "a{background:url(../img/link.gif)}"),
        ("img/bg.gif", "GIF8bg"),
        ("img/link.gif", "GIF8link"),
    ]);
    let r = resurrect(&m, &mock_request()).unwrap();
    assert_eq!(
        included(&r.certificate),
        [
            "css/more.css",
            "css/site.css",
            "img/bg.gif",
            "img/link.gif",
            "p.html"
        ]
    );
    let bg = record(&r.certificate, "img/bg.gif");
    assert_eq!(bg.found_in.as_deref(), Some("css/site.css"));
    let link = record(&r.certificate, "img/link.gif");
    assert_eq!(link.found_in.as_deref(), Some("css/more.css"));
    assert_eq!(record(&r.certificate, "css/site.css").found_in, None);
}

#[test]
fn an_import_is_a_stylesheet_even_without_a_css_extension() {
    let m = site(&[
        ("p.html", r#"<link rel="stylesheet" href="a.css">"#),
        ("a.css", "@import url(theme.txt);"),
        ("theme.txt", "p{background:url(t.gif)}"),
        ("t.gif", "GIF8t"),
    ]);
    let r = resurrect(&m, &mock_request()).unwrap();
    assert!(
        included(&r.certificate).contains(&"t.gif"),
        "{:?}",
        included(&r.certificate)
    );
}

#[test]
fn the_stylesheet_in_the_zip_is_rewritten_with_the_final_restored_set() {
    // gone.gif is not archived, so the reference must be kept as written. bg.gif is, and an
    // absolute reference to it must become relative.
    let m = site(&[
        ("p.html", r#"<link rel="stylesheet" href="css/site.css">"#),
        (
            "css/site.css",
            "a{background:url(http://www.s.org/img/bg.gif)} b{background:url(gone.gif)}",
        ),
        ("img/bg.gif", "GIF8bg"),
    ]);
    let r = resurrect(&m, &mock_request()).unwrap();
    let css = r.entries.iter().find(|e| e.path == "css/site.css").unwrap();
    assert_eq!(
        String::from_utf8_lossy(&css.bytes),
        "a{background:url(../img/bg.gif)} b{background:url(gone.gif)}"
    );
    // The certificate hash is of the rewritten stylesheet (R9.8).
    let rec = record(&r.certificate, "css/site.css");
    assert_eq!(
        rec.sha256.as_deref(),
        Some(revenant_core::package::sha256_hex(&css.bytes).as_str())
    );
    assert_eq!(rec.bytes, Some(css.bytes.len()));
    assert_eq!(record(&r.certificate, "css/gone.gif").verdict, "MISSING");
}

#[test]
fn a_stylesheet_that_imports_itself_is_read_once() {
    let m = site(&[
        ("p.html", r#"<link rel="stylesheet" href="a.css">"#),
        ("a.css", "@import 'b.css'; a{color:red}"),
        ("b.css", "@import 'a.css'; b{color:red}"),
    ]);
    let r = resurrect(&m, &mock_request()).unwrap();
    assert_eq!(included(&r.certificate), ["a.css", "b.css", "p.html"]);
    assert_eq!(
        r.certificate
            .files
            .iter()
            .filter(|f| f.path == "a.css")
            .count(),
        1
    );
}

#[test]
fn imports_stop_at_the_depth_limit_but_images_are_still_followed() {
    // p -> c1 -> c2 -> c3 -> c4. c4 would be depth 4, so it is not fetched. c3's image is.
    let m = site(&[
        ("p.html", r#"<link rel="stylesheet" href="c1.css">"#),
        ("c1.css", "@import 'c2.css';"),
        ("c2.css", "@import 'c3.css';"),
        ("c3.css", "@import 'c4.css'; a{background:url(deep.gif)}"),
        ("c4.css", "z{color:red}"),
        ("deep.gif", "GIF8d"),
    ]);
    let r = resurrect(&m, &mock_request()).unwrap();
    let got = included(&r.certificate);
    assert!(
        got.contains(&"c3.css") && got.contains(&"deep.gif"),
        "{got:?}"
    );
    assert!(!got.contains(&"c4.css"), "{got:?}");
    // The reference is left as written in c3.css, so the stylesheet still says what it wanted.
    let c3 = r.entries.iter().find(|e| e.path == "c3.css").unwrap();
    assert!(String::from_utf8_lossy(&c3.bytes).contains("c4.css"));
}

#[test]
fn the_asset_cap_counts_files_found_inside_stylesheets() {
    // The page names one stylesheet; the stylesheet names 11 images. A cap of 10 must trip.
    let imgs: String = (0..11)
        .map(|i| format!("a{i}{{background:url(i{i}.gif)}}"))
        .collect();
    let m = site(&[
        ("p.html", r#"<link rel="stylesheet" href="s.css">"#),
        ("s.css", imgs.as_str()),
    ]);
    let err = resurrect_with(&m, &mock_request(), Limits::new(10, 1 << 20)).unwrap_err();
    assert_eq!(err, PipelineError::TooManyAssets { found: 11, cap: 10 });
}

#[test]
fn style_blocks_and_attributes_in_the_page_feed_the_same_worklist() {
    let m = site(&[
        (
            "p.html",
            r#"<style>@import 'base.css';</style><td style="background:url(tile.gif)">x</td>"#,
        ),
        ("base.css", "p{background:url(p.gif)}"),
        ("tile.gif", "GIF8t"),
        ("p.gif", "GIF8p"),
    ]);
    let r = resurrect(&m, &mock_request()).unwrap();
    assert_eq!(
        included(&r.certificate),
        ["base.css", "p.gif", "p.html", "tile.gif"]
    );
}

#[test]
fn script_running_css_is_neutralised_everywhere_and_counted_once() {
    let m = site(&[
        (
            "p.html",
            r#"<link rel="stylesheet" href="s.css"><div style="behavior:url(x.htc)">y</div>"#,
        ),
        ("s.css", "a{width:expression(alert(1))}"),
    ]);
    let r = resurrect(&m, &mock_request()).unwrap();
    assert_eq!(r.certificate.css_constructs_neutralised, 2);
    for e in &r.entries {
        let t = String::from_utf8_lossy(&e.bytes);
        if e.path != revenant_core::pipeline::CERTIFICATE_PATH {
            assert!(
                !t.contains("expression") && !t.contains("behavior"),
                "{}: {t}",
                e.path
            );
        }
    }
}

#[test]
fn the_same_stylesheet_restore_gives_the_same_zip() {
    let build = || {
        site(&[
            ("p.html", r#"<link rel="stylesheet" href="s.css">"#),
            ("s.css", "a{background:url(i.gif)}"),
            ("i.gif", "GIF8i"),
        ])
    };
    let a = resurrect(&build(), &mock_request()).unwrap();
    let b = resurrect(&build(), &mock_request()).unwrap();
    assert_eq!(a.zip, b.zip);
}
