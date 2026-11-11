//! Property tests for ZIP packaging. See `.kiro/steering/testing.md`.

use proptest::prelude::*;
use revenant_core::package::{build_zip, sanitize_path, Entry};

proptest! {
    // Any path that sanitization accepts is a safe relative path.
    #[test]
    fn accepted_paths_are_safe(raw in ".*") {
        if let Ok(p) = sanitize_path(&raw) {
            prop_assert!(!p.starts_with('/'));
            prop_assert!(!p.contains('\\'));
            prop_assert!(!p.contains(':'));
            prop_assert!(p.split('/').all(|s| !s.is_empty() && s != "." && s != ".."));
            prop_assert!(!p.chars().any(|c| c.is_control()));
        }
    }

    // Traversal is always rejected, wherever it appears in the path.
    #[test]
    fn traversal_is_always_rejected(prefix in "[a-z]{0,5}", suffix in "[a-z]{0,5}") {
        let raw = format!("{prefix}/../{suffix}");
        prop_assert!(sanitize_path(&raw).is_err());
    }

    // The same entries produce the same archive bytes, whatever the input order.
    #[test]
    fn zip_is_order_independent(
        names in prop::collection::btree_set("[a-z]{1,6}(/[a-z]{1,6}){0,2}", 1..6),
        seed in any::<u64>(),
    ) {
        let entries: Vec<Entry> = names
            .iter()
            .enumerate()
            .map(|(i, n)| Entry { path: n.clone(), bytes: vec![i as u8; i + 1] })
            .collect();
        let forward = build_zip(&entries).expect("packages");
        // A deterministic shuffle driven by the seed.
        let mut shuffled = entries.clone();
        let len = shuffled.len();
        for i in 0..len {
            let j = (seed.wrapping_mul(31).wrapping_add(i as u64) as usize) % len;
            shuffled.swap(i, j);
        }
        prop_assert_eq!(forward, build_zip(&shuffled).expect("packages"));
    }
}
