//! Property tests for the CSS reader (R9.9).

use std::collections::BTreeSet;

use proptest::prelude::*;
use revenant_core::css::rewrite_css;
use revenant_core::rewrite::Context;

fn ctx(restored: &BTreeSet<String>) -> Context<'_> {
    Context {
        page_path: "css/site.css",
        restored,
        site_hosts: &["y2k.gov"],
    }
}

/// CSS-shaped noise: the tokens the reader cares about, mixed with arbitrary text.
fn css_like() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("url(".to_string()),
            Just("url('".to_string()),
            Just("url(\"".to_string()),
            Just(")".to_string()),
            Just("'".to_string()),
            Just("\"".to_string()),
            Just("@import ".to_string()),
            Just("/*".to_string()),
            Just("*/".to_string()),
            Just("expression(".to_string()),
            Just("behavior:".to_string()),
            Just("\\".to_string()),
            Just("\n".to_string()),
            Just(";".to_string()),
            Just(" ".to_string()),
            "[a-z./-]{1,8}",
            "[ -~]{0,6}",
        ],
        0..30,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    // R9.9: arbitrary bytes never panic the reader.
    #[test]
    fn never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..300)) {
        let set = BTreeSet::new();
        let _ = rewrite_css(&bytes, &ctx(&set));
    }

    // R9.9: rewriting twice gives the same bytes as rewriting once, for CSS-shaped input.
    #[test]
    fn rewriting_is_idempotent(css in css_like(), restored in prop::collection::btree_set("[a-z]{1,4}\\.(gif|css)", 0..3)) {
        let set: BTreeSet<String> = restored.into_iter().flat_map(|r| [r.clone(), format!("css/{r}")]).collect();
        let once = rewrite_css(css.as_bytes(), &ctx(&set));
        let twice = rewrite_css(&once.bytes, &ctx(&set));
        prop_assert_eq!(once.bytes, twice.bytes);
    }

    // R9.4 and R9.5: whatever the input, the output has no archive host in a url(...) and none of
    // the script-running constructs.
    #[test]
    fn output_has_no_archive_wrapper_and_no_script_constructs(
        css in css_like(),
        stamp in "[0-9]{4,14}",
        flag in prop::sample::select(vec!["", "im_", "cs_"]),
    ) {
        let set = BTreeSet::new();
        let input = format!("{css} a{{background:url(https://web.archive.org/web/{stamp}{flag}/http://y2k.gov/img/x.gif)}} {css}");
        let out = rewrite_css(input.as_bytes(), &ctx(&set));
        let text: String = out.bytes.iter().map(|&b| b as char).collect();
        // The wrapped URL written by this test is always well formed, so it must be unwrapped,
        // unless the generated noise swallowed it into a comment or string.
        let swallowed = css.contains("/*") || css.contains('\'') || css.contains('"') || css.contains("url(") || css.contains("@import");
        if !swallowed {
            prop_assert!(!text.contains("web.archive.org"), "{text}");
        }
        // `expression(` and `behavior:` can survive only inside a comment or a string, or as the
        // tail of a longer identifier (`aexpression(` is not the construct, R9.5). So look for
        // them at a word boundary.
        if !swallowed {
            let lower = text.to_ascii_lowercase();
            let at_boundary = |needle: &str| {
                lower.match_indices(needle).any(|(i, _)| {
                    i == 0 || !lower.as_bytes()[i - 1].is_ascii_alphanumeric() && lower.as_bytes()[i - 1] != b'-' && lower.as_bytes()[i - 1] != b'_'
                })
            };
            prop_assert!(!at_boundary("expression("), "{text}");
            prop_assert!(!at_boundary("behavior:"), "{text}");
        }
    }

    // A URL reference that was rewritten is still a single token: the number of url( openers
    // never grows, so nothing a reference contains can start a new one.
    #[test]
    fn rewriting_never_adds_url_openers(css in css_like()) {
        let set = BTreeSet::new();
        let out = rewrite_css(css.as_bytes(), &ctx(&set));
        let text: String = out.bytes.iter().map(|&b| b as char).collect();
        prop_assert!(text.matches("url(").count() <= css.matches("url(").count());
    }
}
