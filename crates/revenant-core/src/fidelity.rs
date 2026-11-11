//! Fidelity classifier: decides how trustworthy one file's capture is for a target time.
//!
//! Verdict rules follow `.kiro/specs/revenant-core-engine/requirements.md` (R2, R3, R6).

use crate::time::format_cdx_timestamp;

/// One archived copy of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    /// Capture time as Unix seconds (UTC).
    pub ts: i64,
    /// Content digest from the archive index.
    pub digest: String,
    /// SHA-256 of the served bytes, when they have been fetched.
    pub content: Option<String>,
    /// The origin server's own Last-Modified time, when the archive preserved it (R6.1).
    pub origin_modified: Option<i64>,
}

impl Capture {
    /// A capture with no content hash and no server date.
    pub fn new(ts: i64, digest: impl Into<String>) -> Self {
        Self {
            ts,
            digest: digest.into(),
            content: None,
            origin_modified: None,
        }
    }

    /// True when both captures hold the same content. Compares content hashes when both
    /// are known, and the archive's digests otherwise (R4).
    pub fn same_content(&self, other: &Capture) -> bool {
        match (&self.content, &other.content) {
            (Some(a), Some(b)) => a == b,
            _ => self.digest == other.digest,
        }
    }
}

/// How much a file's chosen capture can be trusted as the version at the target time.
///
/// Ordering runs from most to least trustworthy, so the best verdict is the smallest value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// Unchanged across the target: the captures before and after have the same content.
    Proven,
    /// The only capture is after the target, but the origin server says the file had not changed
    /// since before the target (R6.2).
    Attested,
    /// Captured before the target, and no later capture exists.
    Likely,
    /// Captures exist on both sides of the target, and their content differs.
    Uncertain,
    /// The only capture is after the target, so the file may not have existed as captured then.
    Anachronism,
    /// The file has no captures.
    Missing,
    /// The archive could not be asked, so nothing is known. Never produced by `classify`.
    Unknown,
}

impl Verdict {
    /// Stable upper-case name, used in the certificate and the UI.
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Proven => "PROVEN",
            Verdict::Attested => "ATTESTED",
            Verdict::Likely => "LIKELY",
            Verdict::Uncertain => "UNCERTAIN",
            Verdict::Anachronism => "ANACHRONISM",
            Verdict::Missing => "MISSING",
            Verdict::Unknown => "UNKNOWN",
        }
    }

    /// True for the verdicts that count toward Fidelity (R6.5).
    pub fn is_trusted(self) -> bool {
        matches!(self, Verdict::Proven | Verdict::Attested | Verdict::Likely)
    }
}

/// Why a verdict was reached (R6.7). A fixed vocabulary, so a ledger can always say why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    NoCapture,
    SameContentBothSides,
    LastCaptureBefore,
    CapturesDiffer,
    OnlyLaterCapture,
    /// The server said the file last changed at this time (Unix seconds).
    ServerLastModified(i64),
    /// The archive could not be asked.
    NotAnswered,
}

impl Basis {
    /// A plain sentence for the ledger.
    pub fn text(self) -> String {
        match self {
            Basis::NoCapture => "the archive holds no usable copy".into(),
            Basis::SameContentBothSides => "the same content was captured before and after".into(),
            Basis::LastCaptureBefore => "captured before, and nothing later contradicts it".into(),
            Basis::CapturesDiffer => "captures before and after differ".into(),
            Basis::OnlyLaterCapture => "the only copy was captured after".into(),
            Basis::ServerLastModified(t) => {
                let d = format_cdx_timestamp(t);
                format!(
                    "the server says it had not changed since {}-{}-{}",
                    &d[0..4],
                    &d[4..6],
                    &d[6..8]
                )
            }
            Basis::NotAnswered => "the archive did not answer".into(),
        }
    }
}

/// The chosen capture for one file and the verdict on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub verdict: Verdict,
    pub capture: Option<Capture>,
    /// Chosen capture minus the target, in seconds. Negative means before the target.
    pub delta_secs: Option<i64>,
    pub basis: Basis,
    /// Set when server evidence was ignored or settled a question (R6.3, R6.4).
    pub note: Option<&'static str>,
}

enum Attest {
    /// The server shows the later capture's content already existed at the target.
    Yes(i64),
    /// No usable server evidence.
    No,
    /// The header contradicts something the archive saw, so it is ignored (R6.3).
    Ignored(&'static str),
    /// The server shows the later capture's content was made after the target (R6.4).
    MadeLater,
}

fn attest(target: i64, before: Option<&Capture>, after: &Capture) -> Attest {
    let Some(modified) = after.origin_modified else {
        return Attest::No;
    };
    if modified > after.ts {
        return Attest::Ignored("the server's Last-Modified is later than the capture; ignored");
    }
    if modified > target {
        return Attest::MadeLater;
    }
    if let Some(b) = before {
        if modified <= b.ts && !b.same_content(after) {
            return Attest::Ignored(
                "the server's Last-Modified contradicts the archive's own copies; ignored",
            );
        }
    }
    Attest::Yes(modified)
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
        (Some(b), Some(a)) if a.same_content(b) => select(
            Verdict::Proven,
            Basis::SameContentBothSides,
            b,
            target,
            None,
        ),
        (Some(b), Some(a)) => match attest(target, Some(b), a) {
            Attest::Yes(m) => select(
                Verdict::Attested,
                Basis::ServerLastModified(m),
                a,
                target,
                None,
            ),
            Attest::Ignored(note) => select(
                Verdict::Uncertain,
                Basis::CapturesDiffer,
                b,
                target,
                Some(note),
            ),
            Attest::No | Attest::MadeLater => {
                select(Verdict::Uncertain, Basis::CapturesDiffer, b, target, None)
            }
        },
        (Some(b), None) => select(Verdict::Likely, Basis::LastCaptureBefore, b, target, None),
        (None, Some(a)) => match attest(target, None, a) {
            Attest::Yes(m) => select(
                Verdict::Attested,
                Basis::ServerLastModified(m),
                a,
                target,
                None,
            ),
            Attest::MadeLater => select(
                Verdict::Anachronism,
                Basis::OnlyLaterCapture,
                a,
                target,
                Some("the server shows this version was made after the date"),
            ),
            Attest::Ignored(note) => select(
                Verdict::Anachronism,
                Basis::OnlyLaterCapture,
                a,
                target,
                Some(note),
            ),
            Attest::No => select(
                Verdict::Anachronism,
                Basis::OnlyLaterCapture,
                a,
                target,
                None,
            ),
        },
        (None, None) => Selection {
            verdict: Verdict::Missing,
            capture: None,
            delta_secs: None,
            basis: Basis::NoCapture,
            note: None,
        },
    }
}

/// The capture closest in time to the target, ties going to the earlier one (R7.1).
pub fn nearest(target: i64, captures: &[Capture]) -> Option<&Capture> {
    captures
        .iter()
        .min_by_key(|c| ((c.ts - target).abs(), c.ts, c.digest.clone()))
}

fn select(
    verdict: Verdict,
    basis: Basis,
    capture: &Capture,
    target: i64,
    note: Option<&'static str>,
) -> Selection {
    Selection {
        verdict,
        capture: Some(capture.clone()),
        delta_secs: Some(capture.ts - target),
        basis,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{parse_cdx_timestamp, parse_http_date};

    fn cap(cdx_ts: &str, digest: &str) -> Capture {
        Capture::new(
            parse_cdx_timestamp(cdx_ts).expect("valid CDX timestamp"),
            digest,
        )
    }

    /// A capture whose server said it was last modified at `http`.
    fn cap_lm(cdx_ts: &str, digest: &str, http: &str) -> Capture {
        let mut c = cap(cdx_ts, digest);
        c.origin_modified = Some(parse_http_date(http).expect("valid HTTP date"));
        c
    }

    fn target() -> i64 {
        parse_cdx_timestamp("19991231000000").expect("valid CDX timestamp")
    }

    #[test]
    fn no_captures_is_missing() {
        let s = classify(target(), &[]);
        assert_eq!(s.verdict, Verdict::Missing);
        assert!(s.capture.is_none());
        assert!(s.delta_secs.is_none());
        assert_eq!(s.basis, Basis::NoCapture);
    }

    #[test]
    fn same_digest_on_both_sides_is_proven() {
        let caps = [cap("19991013021843", "X"), cap("20000302030040", "X")];
        let s = classify(target(), &caps);
        assert_eq!(s.verdict, Verdict::Proven);
        assert_eq!(s.basis, Basis::SameContentBothSides);
    }

    #[test]
    fn only_a_capture_before_target_is_likely() {
        let caps = [cap("19991013021843", "X")];
        assert_eq!(classify(target(), &caps).verdict, Verdict::Likely);
    }

    #[test]
    fn different_digests_across_target_is_uncertain() {
        let caps = [cap("19991013021843", "H7C6"), cap("20000302030040", "TUVG")];
        let s = classify(target(), &caps);
        assert_eq!(s.verdict, Verdict::Uncertain);
        assert_eq!(s.capture.expect("chosen capture").digest, "H7C6");
    }

    #[test]
    fn only_a_capture_after_target_is_anachronism() {
        let s = classify(target(), &[cap("20001013054719", "A")]);
        assert_eq!(s.verdict, Verdict::Anachronism);
        assert!(s.delta_secs.expect("delta") > 0);
        assert_eq!(s.note, None);
    }

    #[test]
    fn equal_content_overrides_different_digests() {
        let mut a = cap("19991013021843", "D1");
        let mut b = cap("20000302030040", "D2");
        a.content = Some("same".into());
        b.content = Some("same".into());
        assert_eq!(classify(target(), &[a, b]).verdict, Verdict::Proven);
    }

    #[test]
    fn different_content_overrides_equal_digests() {
        let mut a = cap("19991013021843", "D");
        let mut b = cap("20000302030040", "D");
        a.content = Some("one".into());
        b.content = Some("two".into());
        assert_eq!(classify(target(), &[a, b]).verdict, Verdict::Uncertain);
    }

    #[test]
    fn delta_is_negative_before_target() {
        let s = classify(target(), &[cap("19991013021843", "X")]);
        assert!(s.delta_secs.expect("delta") < 0);
    }

    #[test]
    fn verdict_order_runs_best_to_worst() {
        let order = [
            Verdict::Proven,
            Verdict::Attested,
            Verdict::Likely,
            Verdict::Uncertain,
            Verdict::Anachronism,
            Verdict::Missing,
            Verdict::Unknown,
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]));
    }

    // ---- R6: the real y2k.gov facts that motivated server evidence ----

    /// images/1.gif: captured 2000-03-04, the server says it last changed 1999-12-27.
    #[test]
    fn a_later_capture_the_server_says_was_unchanged_is_attested() {
        let t = parse_cdx_timestamp("20000302235959").unwrap();
        let img = cap_lm("20000304045622", "I", "Mon, 27 Dec 1999 19:41:03 GMT");
        let s = classify(t, &[img]);
        assert_eq!(s.verdict, Verdict::Attested);
        assert_eq!(
            s.basis,
            Basis::ServerLastModified(parse_http_date("Mon, 27 Dec 1999 19:41:03 GMT").unwrap())
        );
        assert!(s.basis.text().contains("1999-12-27"), "{}", s.basis.text());
        assert!(
            s.delta_secs.unwrap() > 0,
            "the chosen capture is still the later one"
        );
    }

    /// briefings.html: copy A (2000-03-01), copy B (2000-03-04, last modified 2000-03-02 01:06).
    #[test]
    fn the_page_replaced_during_the_day_is_attested_at_the_end_of_that_day() {
        let a = cap_lm("20000301091452", "A", "Fri, 25 Feb 2000 19:22:45 GMT");
        let b = cap_lm("20000304041110", "B", "Thu, 02 Mar 2000 01:06:59 GMT");
        let end = parse_cdx_timestamp("20000302235959").unwrap();
        let s = classify(end, &[a.clone(), b.clone()]);
        assert_eq!(s.verdict, Verdict::Attested);
        assert_eq!(s.capture.unwrap().digest, "B", "copy A was already stale");

        // At midnight the replacement had not happened yet, and the archive cannot say whether
        // the change came before or after, so the honest answer is still Uncertain.
        let midnight = parse_cdx_timestamp("20000302000000").unwrap();
        let s = classify(midnight, &[a, b]);
        assert_eq!(s.verdict, Verdict::Uncertain);
        assert_eq!(s.capture.unwrap().digest, "A");
    }

    #[test]
    fn a_version_the_server_shows_was_made_later_is_a_confirmed_anachronism() {
        let t = parse_cdx_timestamp("19990101000000").unwrap();
        let page = cap_lm("20000301091452", "A", "Fri, 25 Feb 2000 19:22:45 GMT");
        let s = classify(t, &[page]);
        assert_eq!(s.verdict, Verdict::Anachronism);
        assert_eq!(
            s.note,
            Some("the server shows this version was made after the date")
        );
    }

    #[test]
    fn a_header_later_than_its_own_capture_is_ignored() {
        let t = parse_cdx_timestamp("20000302235959").unwrap();
        // Captured 2000-03-04, but claims it was modified 2000-03-09: impossible.
        let c = cap_lm("20000304045622", "I", "Thu, 09 Mar 2000 00:00:00 GMT");
        let s = classify(t, &[c]);
        assert_eq!(s.verdict, Verdict::Anachronism);
        assert!(s.note.unwrap().contains("later than the capture"));
    }

    #[test]
    fn a_header_that_contradicts_an_earlier_capture_is_ignored() {
        let t = parse_cdx_timestamp("20000302235959").unwrap();
        // The archive saw content A on Mar 1. The later copy B claims it last changed on Feb 20,
        // which would mean B was already live on Mar 1. The two cannot both be right.
        let a = cap("20000301091452", "A");
        let b = cap_lm("20000304041110", "B", "Sun, 20 Feb 2000 00:00:00 GMT");
        let s = classify(t, &[a, b]);
        assert_eq!(s.verdict, Verdict::Uncertain);
        assert!(s.note.unwrap().contains("contradicts"));
    }

    #[test]
    fn a_dynamic_page_that_stamps_the_capture_time_gives_no_false_evidence() {
        let t = parse_cdx_timestamp("20000302235959").unwrap();
        let mut c = cap("20000304041110", "D");
        c.origin_modified = Some(c.ts); // Last-Modified equals the request time
        let s = classify(t, &[c]);
        assert_eq!(s.verdict, Verdict::Anachronism);
    }

    #[test]
    fn server_evidence_never_changes_proven_likely_or_missing() {
        let t = parse_cdx_timestamp("20000302235959").unwrap();
        let mut proven = [cap("20000301000000", "X"), cap("20000304000000", "X")];
        proven[1].origin_modified = Some(parse_cdx_timestamp("19990101000000").unwrap());
        assert_eq!(classify(t, &proven).verdict, Verdict::Proven);

        let mut likely = cap("20000301000000", "X");
        likely.origin_modified = Some(parse_cdx_timestamp("19990101000000").unwrap());
        assert_eq!(classify(t, &[likely]).verdict, Verdict::Likely);
    }

    #[test]
    fn nearest_takes_the_closest_capture_and_the_earlier_on_a_tie() {
        let t = parse_cdx_timestamp("20000302000000").unwrap();
        let near_after = cap("20000303000000", "N"); // 1 day after
        let far_before = cap("20000227000000", "F"); // 4 days before
        assert_eq!(
            nearest(t, &[far_before.clone(), near_after.clone()])
                .unwrap()
                .digest,
            "N"
        );

        let tie_before = cap("20000301000000", "B"); // 1 day before
        assert_eq!(nearest(t, &[near_after, tie_before]).unwrap().digest, "B");
        assert!(nearest(t, &[]).is_none());
    }
}
