//! Fixture test: `briefings.html` on y2k.gov, restored as of 2000-03-02. The index lives in
//! `fixtures/snapshots/y2k-briefings/captures.tsv`. Expected values are in `docs/spike/m1-y2k.md`.

use revenant_core::autopsy::autopsy;
use revenant_core::fidelity::{classify, Verdict};
use revenant_core::index::parse_index;
use revenant_core::time::parse_cdx_timestamp;

const INDEX: &str = include_str!("../../../fixtures/snapshots/y2k-briefings/captures.tsv");

const REFERENCED: [&str; 11] = [
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

fn target() -> i64 {
    parse_cdx_timestamp("20000302000000").expect("valid timestamp")
}

#[test]
fn the_page_itself_is_uncertain_between_march_1_and_march_4() {
    let idx = parse_index(INDEX).expect("index parses");
    let s = classify(target(), idx.captures("y2k.gov/briefings.html"));
    assert_eq!(s.verdict, Verdict::Uncertain);
    assert_eq!(
        s.capture.expect("chosen").ts,
        parse_cdx_timestamp("20000301091452").unwrap()
    );
}

#[test]
fn images_first_archived_on_march_4_are_anachronisms_at_march_2() {
    let idx = parse_index(INDEX).expect("index parses");
    for n in [1, 3, 4, 5, 6, 7] {
        let label = format!("y2k.gov/images/{n}.gif");
        let s = classify(target(), idx.captures(&label));
        assert_eq!(s.verdict, Verdict::Anachronism, "{label}");
        // Each is archived on 2000-03-04, two days after the target.
        let days = s.delta_secs.expect("delta") / 86_400;
        assert_eq!(days, 2, "{label} is {days} days after the target");
    }
}

#[test]
fn images_without_any_200_capture_are_missing() {
    let idx = parse_index(INDEX).expect("index parses");
    for label in [
        "y2k.gov/images/8.gif",
        "y2k.gov/images/9.gif",
        "y2k.gov/images/2_briefings.gif",
        "y2k.gov/images/briefings_title1.gif",
        "y2k.gov/images/briefings_title2.gif",
    ] {
        assert_eq!(
            classify(target(), idx.captures(label)).verdict,
            Verdict::Missing,
            "{label}"
        );
    }
}

#[test]
fn autopsy_at_march_2_reports_completeness_and_zero_fidelity() {
    let idx = parse_index(INDEX).expect("index parses");
    let mut files = vec!["y2k.gov/briefings.html"];
    files.extend_from_slice(&REFERENCED);
    let a = autopsy(target(), &files, &idx);
    // Recovered (any verdict but Missing): the page plus images 1, 3, 4, 5, 6, 7 = 7 of 12 files.
    let c = a.completeness.expect("has files");
    assert!((c - 7.0 * 100.0 / 12.0).abs() < 1e-9, "completeness {c}");
    // Nothing is Proven or Likely, so fidelity is 0 of the 7 recovered files.
    assert_eq!(a.fidelity, Some(0.0));
}
