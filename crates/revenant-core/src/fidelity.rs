//! Fidelity classifier: decides how trustworthy one file's capture is for a target time.
//!
//! Verdict rules follow `.kiro/specs/revenant-core-engine/requirements.md` (R2, R3).

/// One archived copy of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    /// Capture time as Unix seconds (UTC).
    pub ts: i64,
    /// Content digest from the archive index.
    pub digest: String,
}

/// How much a file's chosen capture can be trusted as the version at the target time.
///
/// Ordering runs from most to least trustworthy, so the best verdict is the smallest value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// Unchanged across the target: the captures before and after have the same digest.
    Proven,
    /// Captured before the target, and no later capture exists.
    Likely,
    /// Captures exist on both sides of the target, and their digests differ.
    Uncertain,
    /// The only capture is after the target, so the file did not exist as captured then.
    Anachronism,
    /// The file has no captures.
    Missing,
}

/// The chosen capture for one file and the verdict on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub verdict: Verdict,
    pub capture: Option<Capture>,
    /// Chosen capture time minus the target, in seconds. Negative means before the target.
    pub delta_secs: Option<i64>,
}

/// Classify one file's captures against a target time.
///
/// The result does not depend on the order of `captures`.
pub fn classify(target: i64, captures: &[Capture]) -> Selection {
    let mut sorted: Vec<&Capture> = captures.iter().collect();
    sorted.sort_by(|a, b| (a.ts, &a.digest).cmp(&(b.ts, &b.digest)));

    let before = sorted.iter().rev().find(|c| c.ts <= target).copied();
    let after = sorted.iter().find(|c| c.ts > target).copied();

    match (before, after) {
        (Some(b), Some(a)) if a.digest == b.digest => select(Verdict::Proven, b, target),
        (Some(b), None) => select(Verdict::Likely, b, target),
        (Some(b), Some(_)) => select(Verdict::Uncertain, b, target),
        (None, Some(a)) => select(Verdict::Anachronism, a, target),
        (None, None) => Selection {
            verdict: Verdict::Missing,
            capture: None,
            delta_secs: None,
        },
    }
}

fn select(verdict: Verdict, capture: &Capture, target: i64) -> Selection {
    Selection {
        verdict,
        capture: Some(capture.clone()),
        delta_secs: Some(capture.ts - target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::parse_cdx_timestamp;

    fn cap(cdx_ts: &str, digest: &str) -> Capture {
        Capture {
            ts: parse_cdx_timestamp(cdx_ts).expect("valid CDX timestamp"),
            digest: digest.to_string(),
        }
    }

    fn target() -> i64 {
        parse_cdx_timestamp("19991231000000").expect("valid CDX timestamp")
    }

    // Real captures from docs/spike/m1-y2k.md.

    #[test]
    fn no_captures_is_missing() {
        let s = classify(target(), &[]);
        assert_eq!(s.verdict, Verdict::Missing);
        assert!(s.capture.is_none());
        assert!(s.delta_secs.is_none());
    }

    #[test]
    fn same_digest_on_both_sides_is_proven() {
        let caps = [cap("19991013021843", "X"), cap("20000302030040", "X")];
        assert_eq!(classify(target(), &caps).verdict, Verdict::Proven);
    }

    #[test]
    fn only_a_capture_before_target_is_likely() {
        let caps = [cap("19991013021843", "X")];
        assert_eq!(classify(target(), &caps).verdict, Verdict::Likely);
    }

    #[test]
    fn different_digests_across_target_is_uncertain() {
        // The y2k.gov root page: 1999-10-13 (H7C6) vs 2000-03-02 (TUVG).
        let caps = [cap("19991013021843", "H7C6"), cap("20000302030040", "TUVG")];
        let s = classify(target(), &caps);
        assert_eq!(s.verdict, Verdict::Uncertain);
        assert_eq!(s.capture.expect("chosen capture").digest, "H7C6");
    }

    #[test]
    fn only_a_capture_after_target_is_anachronism() {
        // Synthetic: a single 200 capture after the target. Real y2k GIFs have no 200 captures
        // and classify as Missing (see tests/fixture_y2k.rs).
        let s = classify(target(), &[cap("20001013054719", "A")]);
        assert_eq!(s.verdict, Verdict::Anachronism);
        assert!(s.delta_secs.expect("delta") > 0);
    }

    #[test]
    fn delta_is_negative_before_target() {
        let s = classify(target(), &[cap("19991013021843", "X")]);
        assert!(s.delta_secs.expect("delta") < 0);
    }

    #[test]
    fn verdict_order_runs_best_to_worst() {
        assert!(Verdict::Proven < Verdict::Likely);
        assert!(Verdict::Likely < Verdict::Uncertain);
        assert!(Verdict::Uncertain < Verdict::Anachronism);
        assert!(Verdict::Anachronism < Verdict::Missing);
    }
}
