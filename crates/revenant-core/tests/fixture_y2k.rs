//! Fixture test: the engine run on the captured y2k.gov index gives the verdicts recorded in
//! `docs/spike/m1-y2k.md`. The index lives in `fixtures/snapshots/y2k/captures.tsv`.

use revenant_core::fidelity::{classify, Verdict};
use revenant_core::index::parse_index;
use revenant_core::time::parse_cdx_timestamp;

const INDEX: &str = include_str!("../../../fixtures/snapshots/y2k/captures.tsv");

fn target() -> i64 {
    parse_cdx_timestamp("19991231000000").expect("valid timestamp")
}

#[test]
fn homepage_at_1999_12_31_is_uncertain_and_uses_the_october_capture() {
    let idx = parse_index(INDEX).expect("index parses");
    let s = classify(target(), idx.captures("y2k.gov/"));
    assert_eq!(s.verdict, Verdict::Uncertain);
    assert_eq!(
        s.capture.expect("chosen capture").digest,
        "H7C62LRXIRKNLG76EMHU6MLZF767VVZQ"
    );
}

#[test]
fn gifs_archived_only_as_404_are_missing() {
    let idx = parse_index(INDEX).expect("index parses");
    for label in [
        "y2k.gov/gif/award.gif",
        "y2k.gov/gif/award2.gif",
        "y2k.gov/gif/linkwht.gif",
        "y2k.gov/gif/splasha.gif",
        "y2k.gov/gif/splashb.gif",
    ] {
        assert!(idx.captures(label).is_empty(), "{label} has 200 captures");
        assert_eq!(
            classify(target(), idx.captures(label)).verdict,
            Verdict::Missing
        );
    }
}

#[test]
fn index_drops_the_404_rows_it_was_given() {
    let idx = parse_index(INDEX).expect("index parses");
    // 29 root 404s, plus 1 + 1 + 1 + 6 + 2 GIF 404s.
    assert_eq!(idx.dropped_non_200(), 40);
}
