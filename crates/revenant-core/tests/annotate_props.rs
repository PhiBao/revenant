//! Property tests for the annotated view (R8.6, R8.9).

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use revenant_core::annotate::{annotate_html, Annotation, FileNote};

fn annotation() -> Annotation {
    let mut files = BTreeMap::new();
    files.insert(
        "images/a.gif".to_string(),
        FileNote {
            class: "attested",
            tag: "attested 1999-12-27".into(),
            detail: "Attested.".into(),
        },
    );
    files.insert(
        "images/gone.gif".to_string(),
        FileNote {
            class: "missing",
            tag: "not archived".into(),
            detail: "Missing.".into(),
        },
    );
    Annotation {
        page_path: "index.html".into(),
        site: "y2k.gov".into(),
        files,
        restored: BTreeSet::from(["images/a.gif".to_string()]),
        banner: "REVENANT annotated view. This page: attested.".into(),
    }
}

proptest! {
    // R8.9: arbitrary bytes never panic the annotator.
    #[test]
    fn never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..400)) {
        let _ = annotate_html(&bytes, &annotation());
    }

    // R8.6: text taken from the archived page is never copied into the markup the annotator adds.
    // A marker is planted in every attribute the annotator reads, including attributes that break
    // out of their tag. The annotator may drop page text (a placeholder replaces the image) but
    // must never make it appear MORE often than it did in the input: a copy would raise the count.
    #[test]
    fn page_controlled_text_is_never_copied_into_inserted_markup(
        marker in "[A-Za-z]{8,12}",
        width in "[0-9a-z%\" =<>]{0,10}",
        name in prop::sample::select(vec!["images/a.gif", "images/gone.gif"]),
    ) {
        let marker = format!("ZZ{marker}ZZ");
        let html = format!(
            "<body><img src=\"{name}\" alt=\"{marker}\" title=\"{marker}\" width=\"{width}\" height=\"{width}\" class=\"{marker}\"></body>"
        );
        let out = annotate_html(html.as_bytes(), &annotation()).expect("annotates");
        let text: String = out.iter().map(|&b| b as char).collect();
        prop_assert!(
            text.matches(&marker).count() <= html.matches(&marker).count(),
            "page text was copied into the output: {text}"
        );
        // The style block and the bar are added exactly once, whatever the page says.
        prop_assert_eq!(text.matches("id=\"rv-bar\"").count(), 1);
        prop_assert_eq!(text.matches("<style>").count(), 1);
    }

    // R8.7: bytes outside the markup are left alone. Input with no body and no known image comes
    // back byte-for-byte.
    #[test]
    fn input_with_nothing_to_annotate_comes_back_identical(
        bytes in prop::collection::vec(prop::sample::select(vec![b'a', b'b', b' ', 0xE9, 0xFF, b'\n']), 0..120),
    ) {
        let out = annotate_html(&bytes, &annotation()).expect("annotates");
        prop_assert_eq!(out, bytes);
    }
}
