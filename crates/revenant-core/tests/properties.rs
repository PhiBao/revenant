//! Property tests for the fidelity engine. Each property maps to a requirement in
//! `.kiro/specs/revenant-core-engine/requirements.md`.

use proptest::prelude::*;
use revenant_core::fidelity::{classify, Capture, Verdict};
use revenant_core::time::parse_cdx_timestamp;

fn capture_strategy() -> impl Strategy<Value = Capture> {
    // A small time range and digest alphabet, so collisions and same-digest runs are common.
    (0i64..10_000, "[abc]{1,2}").prop_map(|(ts, digest)| Capture {
        ts: ts * 86_400,
        digest,
        content: None,
    })
}

proptest! {
    // R1.3: the parser never panics on arbitrary input.
    #[test]
    fn parser_never_panics(s in ".*") {
        let _ = parse_cdx_timestamp(&s);
    }

    // R3.1: the verdict does not depend on the order of captures.
    #[test]
    fn classification_is_order_invariant(
        caps in prop::collection::vec(capture_strategy(), 0..12),
        target in 0i64..10_000,
    ) {
        let target = target * 86_400;
        let forward = classify(target, &caps);
        let mut reversed = caps.clone();
        reversed.reverse();
        let backward = classify(target, &reversed);
        prop_assert_eq!(forward, backward);
    }

    // R3.2: a capture at or before the target means the verdict is never Anachronism.
    #[test]
    fn no_false_anachronism(
        caps in prop::collection::vec(capture_strategy(), 1..12),
        target in 0i64..10_000,
    ) {
        let target = target * 86_400;
        let has_before = caps.iter().any(|c| c.ts <= target);
        let verdict = classify(target, &caps).verdict;
        if has_before {
            prop_assert_ne!(verdict, Verdict::Anachronism);
        }
        if !has_before && !caps.is_empty() {
            prop_assert_eq!(verdict, Verdict::Anachronism);
        }
    }

    // R3.3: a Proven selection's digest matches a capture on the far side of the target.
    #[test]
    fn proven_digest_matches_a_later_capture(
        caps in prop::collection::vec(capture_strategy(), 1..12),
        target in 0i64..10_000,
    ) {
        let target = target * 86_400;
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
        let target = target * 86_400;
        let s = classify(target, &caps);
        if let (Some(c), Some(d)) = (s.capture, s.delta_secs) {
            prop_assert_eq!(d, c.ts - target);
        }
    }
}
