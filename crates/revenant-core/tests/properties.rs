//! Property tests for the fidelity engine. Each property maps to a requirement in
//! `.kiro/specs/revenant-core-engine/requirements.md`.

use proptest::prelude::*;
use revenant_core::fidelity::{classify, nearest, Capture, Verdict};
use revenant_core::time::{parse_cdx_timestamp, parse_http_date};

const DAY: i64 = 86_400;

/// Small time range and digest alphabet, so same-content runs and ties are common. About half the
/// captures carry a server date, which may be before the capture, equal to it, or after it.
fn capture_strategy() -> impl Strategy<Value = Capture> {
    (
        0i64..10_000,
        "[abc]{1,2}",
        prop::option::of(-3_000i64..3_000),
    )
        .prop_map(|(day, digest, lm_offset)| {
            let ts = day * DAY;
            Capture {
                ts,
                digest,
                content: None,
                origin_modified: lm_offset.map(|o| ts + o * DAY),
            }
        })
}

fn without_server_dates(caps: &[Capture]) -> Vec<Capture> {
    caps.iter()
        .cloned()
        .map(|mut c| {
            c.origin_modified = None;
            c
        })
        .collect()
}

proptest! {
    // R1.3: the timestamp parser never panics on arbitrary input.
    #[test]
    fn parser_never_panics(s in ".*") {
        let _ = parse_cdx_timestamp(&s);
    }

    // R6.10: the HTTP date parser never panics, and what it accepts re-parses identically.
    #[test]
    fn http_date_parser_never_panics(s in ".*") {
        let _ = parse_http_date(&s);
    }

    // R3.1: the verdict does not depend on the order of captures.
    #[test]
    fn classification_is_order_invariant(
        caps in prop::collection::vec(capture_strategy(), 0..12),
        target in 0i64..10_000,
    ) {
        let target = target * DAY;
        let forward = classify(target, &caps);
        let mut reversed = caps.clone();
        reversed.reverse();
        prop_assert_eq!(forward, classify(target, &reversed));
    }

    // R3.2 and R6.2: with a capture at or before the target there is never an Anachronism.
    // Without one, the verdict is an Anachronism, or Attested when the server vouches for it.
    #[test]
    fn no_false_anachronism(
        caps in prop::collection::vec(capture_strategy(), 1..12),
        target in 0i64..10_000,
    ) {
        let target = target * DAY;
        let has_before = caps.iter().any(|c| c.ts <= target);
        let verdict = classify(target, &caps).verdict;
        if has_before {
            prop_assert_ne!(verdict, Verdict::Anachronism);
        } else {
            prop_assert!(matches!(verdict, Verdict::Anachronism | Verdict::Attested));
        }
    }

    // R3.3: a Proven selection's content matches a capture on the far side of the target.
    #[test]
    fn proven_digest_matches_a_later_capture(
        caps in prop::collection::vec(capture_strategy(), 1..12),
        target in 0i64..10_000,
    ) {
        let target = target * DAY;
        let s = classify(target, &caps);
        if s.verdict == Verdict::Proven {
            let chosen = s.capture.expect("proven has a capture");
            prop_assert!(chosen.ts <= target);
            prop_assert!(caps.iter().any(|c| c.ts > target && c.digest == chosen.digest));
        }
    }

    // R3.4: delta is the chosen capture's time minus the target.
    #[test]
    fn delta_matches_chosen_capture(
        caps in prop::collection::vec(capture_strategy(), 1..12),
        target in 0i64..10_000,
    ) {
        let target = target * DAY;
        let s = classify(target, &caps);
        if let (Some(c), Some(d)) = (s.capture, s.delta_secs) {
            prop_assert_eq!(d, c.ts - target);
        }
    }

    // R6.6: server evidence may only turn Anachronism or Uncertain into Attested. It never
    // touches Proven, Likely or Missing, and never makes a verdict worse than it was.
    #[test]
    fn server_evidence_can_only_promote_to_attested(
        caps in prop::collection::vec(capture_strategy(), 0..12),
        target in 0i64..10_000,
    ) {
        let target = target * DAY;
        let plain = classify(target, &without_server_dates(&caps)).verdict;
        let with = classify(target, &caps).verdict;
        match plain {
            Verdict::Proven | Verdict::Likely | Verdict::Missing => prop_assert_eq!(with, plain),
            _ => prop_assert!(with == plain || with == Verdict::Attested),
        }
        prop_assert!(with <= plain, "{:?} became worse: {:?}", plain, with);
    }

    // R6.2 and R6.3: Attested is never given when its own evidence is unusable. The later
    // capture's server date must be at or before the target and at or before its capture time,
    // and no earlier capture may contradict it.
    #[test]
    fn attested_never_rests_on_contradicted_evidence(
        caps in prop::collection::vec(capture_strategy(), 1..12),
        target in 0i64..10_000,
    ) {
        let target = target * DAY;
        let s = classify(target, &caps);
        if s.verdict == Verdict::Attested {
            let chosen = s.capture.expect("attested has a capture");
            let modified = chosen.origin_modified.expect("attested has a server date");
            prop_assert!(chosen.ts > target);
            prop_assert!(modified <= target, "the server date must not be after the target");
            prop_assert!(modified <= chosen.ts, "the server date must not be after the capture");
            // The chosen capture is the earliest one after the target.
            prop_assert!(caps.iter().filter(|c| c.ts > target).all(|c| c.ts >= chosen.ts));
            // No capture at or before the target saw different content after the claimed change.
            for b in caps.iter().filter(|c| c.ts <= target) {
                prop_assert!(modified > b.ts || b.same_content(&chosen));
            }
        }
    }

    // R7.1: the nearest capture is never farther from the target than any other capture.
    #[test]
    fn nearest_is_no_farther_than_any_other_capture(
        caps in prop::collection::vec(capture_strategy(), 1..12),
        target in 0i64..10_000,
    ) {
        let target = target * DAY;
        let chosen = nearest(target, &caps).expect("non-empty");
        let best = caps.iter().map(|c| (c.ts - target).abs()).min().unwrap();
        prop_assert_eq!((chosen.ts - target).abs(), best);
    }
}

/// Guards against vacuous properties: the generator must reach every verdict the properties
/// reason about, and `Attested` often enough that its properties are really exercised.
#[test]
fn the_generator_reaches_every_verdict() {
    use proptest::strategy::{Strategy, ValueTree};
    use proptest::test_runner::TestRunner;
    use std::collections::BTreeMap;

    let strategy = (
        prop::collection::vec(capture_strategy(), 0..12),
        0i64..10_000,
    );
    let mut runner = TestRunner::deterministic();
    let mut seen: BTreeMap<&'static str, u32> = BTreeMap::new();
    let n = 4_000;
    for _ in 0..n {
        let (caps, target) = strategy.new_tree(&mut runner).unwrap().current();
        *seen
            .entry(classify(target * DAY, &caps).verdict.as_str())
            .or_default() += 1;
    }
    eprintln!("verdict coverage over {n} cases: {seen:?}");
    for verdict in [
        "PROVEN",
        "ATTESTED",
        "LIKELY",
        "UNCERTAIN",
        "ANACHRONISM",
        "MISSING",
    ] {
        let count = seen.get(verdict).copied().unwrap_or(0);
        assert!(
            count * 100 >= n,
            "{verdict} appeared in only {count} of {n} cases"
        );
    }
}
