//! Property tests for the HTML rewriter. See `.kiro/steering/testing.md`.

use std::collections::BTreeSet;

use proptest::prelude::*;
use revenant_core::rewrite::{rewrite_html, Context};

fn ctx<'a>(restored: &'a BTreeSet<String>) -> Context<'a> {
    Context {
        page_path: "docs/page.html",
        restored,
        site_hosts: &["y2k.gov"],
    }
}

proptest! {
    // The rewriter never panics, whatever bytes it is given.
    #[test]
    fn never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..400)) {
        let restored = BTreeSet::new();
        let _ = rewrite_html(&bytes, &ctx(&restored));
    }

    // No URL attribute keeps the archive host, however the archive URL is written.
    #[test]
    fn archive_host_never_survives_in_attributes(
        scheme in prop::sample::select(vec!["http://", "https://", "//", "/"]),
        stamp in "[0-9]{4,14}",
        flag in prop::sample::select(vec!["", "im_", "id_", "js_", "cs_"]),
        host in prop::sample::select(vec!["www.y2k.gov:80", "y2k.gov", "www.latimes.com", "example.org"]),
        path in "[a-z]{1,6}(/[a-z]{1,6}){0,2}\\.(gif|html)",
        attr in prop::sample::select(vec!["src", "href", "background"]),
        restore in any::<bool>(),
    ) {
        let prefix = if scheme == "/" { "/web/".to_string() } else { format!("{scheme}web.archive.org/web/") };
        let url = format!("{prefix}{stamp}{flag}/http://{host}/{path}");
        let html = format!("<a {attr}=\"{url}\">x</a>");
        let mut restored = BTreeSet::new();
        if restore { restored.insert(path.clone()); }
        let out = rewrite_html(html.as_bytes(), &ctx(&restored)).expect("rewrites");
        let text: String = out.bytes.iter().map(|&b| b as char).collect();
        prop_assert!(!text.contains("web.archive.org"), "{text}");
    }

    // Scripts and inline handlers never survive.
    #[test]
    fn scripts_and_handlers_never_survive(body in "[a-z ]{0,20}", handler in "on[a-z]{3,8}") {
        let html = format!("<div {handler}=\"x()\"><script>{body}</script></div>");
        let restored = BTreeSet::new();
        let out = rewrite_html(html.as_bytes(), &ctx(&restored)).expect("rewrites");
        let text: String = out.bytes.iter().map(|&b| b as char).collect();
        prop_assert!(!text.to_lowercase().contains("<script"), "{text}");
        prop_assert!(!text.contains(&handler), "{text}");
    }
}
