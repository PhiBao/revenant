//! The annotated view of a restored page (requirement R8).
//!
//! A restored page looks like the original, which hides what Revenant knows about it. This module
//! makes a copy for display: every image is outlined in its verdict's colour with a short tag, an
//! image that was not restored becomes a labelled placeholder instead of a broken-image icon, and
//! a bar at the top says what the page itself is.
//!
//! The ZIP is never annotated. Nothing from the archived page is copied into the markup this module
//! adds: every inserted word comes from the certificate's fixed vocabulary or from its dates, and is
//! escaped. As in the rewriter, bytes are read and written one-to-one, so inserted text is ASCII only.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use lol_html::html_content::{ContentType, Element};
use lol_html::{element, rewrite_str, RewriteStrSettings};

use crate::pipeline::{Certificate, FileRecord};
use crate::rewrite::{site_path_of, RewriteError};

/// What the annotator needs to know about one site file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileNote {
    /// Lower-case verdict name, used as a CSS class.
    pub class: &'static str,
    /// Short text shown on the page. Short enough for a small image (R8.10).
    pub tag: String,
    /// The full sentence, shown as a tooltip.
    pub detail: String,
}

const STYLE: &str = "<style>\
.rv{display:inline-flex;flex-direction:column;align-items:stretch;vertical-align:top;line-height:0;max-width:100%;outline:2px solid var(--rv,#2f9e5b);outline-offset:-2px}\
.rv::after{content:attr(data-rv-tag);display:block;box-sizing:border-box;width:100%;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;\
font:10px/14px monospace;padding:0 3px;color:#e8e3d2;background:var(--rv,#2f9e5b);filter:brightness(.62)}\
.rv-gap{display:inline-block;box-sizing:border-box;min-width:6em;min-height:2.4em;padding:3px 5px;border:2px dashed var(--rv,#6b6759);\
font:11px/1.35 monospace;color:#444;background:#f1efe6;vertical-align:top;overflow:hidden}\
.rv-proven,.rv-attested,.rv-likely{--rv:#2f9e5b}\
.rv-uncertain{--rv:#b8891f}\
.rv-anachronism{--rv:#c0603a}\
.rv-missing,.rv-unknown{--rv:#6b6759}\
#rv-bar{position:sticky;top:0;z-index:2147483647;margin:0;padding:6px 10px;background:#0b0c0a;color:#e8e3d2;font:12px/1.4 monospace;border-bottom:2px solid #7ee0a1}\
</style>";

/// ASCII-only, HTML-safe text. Anything outside printable ASCII becomes `?` (R8.6).
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if (' '..='~').contains(&c) => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

fn day(cdx: Option<&str>) -> String {
    match cdx {
        Some(d) if d.len() >= 8 && d.bytes().take(8).all(|b| b.is_ascii_digit()) => {
            format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8])
        }
        _ => "an unknown date".to_string(),
    }
}

fn note_for(f: &FileRecord) -> FileNote {
    let modified = day(f.origin_modified.as_deref());
    let captured = day(f.capture_time.as_deref());
    let (class, tag, detail) = match f.verdict {
        "PROVEN" => (
            "proven",
            "proven".to_string(),
            "Proven: the same content was captured before and after the date.".to_string(),
        ),
        "ATTESTED" => (
            "attested",
            format!("attested {modified}"),
            format!("Attested: saved after the date, but the server says it had not changed since {modified}."),
        ),
        "LIKELY" => (
            "likely",
            "likely".to_string(),
            "Likely: captured before the date and nothing later contradicts it.".to_string(),
        ),
        "UNCERTAIN" => (
            "uncertain",
            "uncertain".to_string(),
            "Uncertain: the copies before and after the date differ.".to_string(),
        ),
        "ANACHRONISM" if f.included => (
            "anachronism",
            "later than date".to_string(),
            format!("Anachronism: first saved {captured}, after the date."),
        ),
        "ANACHRONISM" => (
            "anachronism",
            format!("first saved {captured}"),
            format!("Left out: the only copy was saved {captured}, after the date."),
        ),
        "MISSING" => (
            "missing",
            "not archived".to_string(),
            "Missing: the archive holds no usable copy.".to_string(),
        ),
        _ => (
            "unknown",
            "no answer".to_string(),
            "Unknown: the archive did not answer, which is not the same as missing.".to_string(),
        ),
    };
    FileNote { class, tag, detail }
}

/// Everything the annotator reads from a certificate.
pub struct Annotation {
    pub page_path: String,
    pub site: String,
    pub files: BTreeMap<String, FileNote>,
    pub restored: BTreeSet<String>,
    pub banner: String,
}

impl Annotation {
    pub fn from_certificate(cert: &Certificate) -> Self {
        let mut files = BTreeMap::new();
        let mut restored = BTreeSet::new();
        for f in &cert.files {
            files.insert(f.path.clone(), note_for(f));
            if f.included {
                restored.insert(f.path.clone());
            }
        }
        let page = cert.files.first();
        let banner = match page {
            Some(p) => {
                let mut b = format!(
                    "REVENANT annotated view. This page: {}. Copy saved {}.",
                    p.verdict.to_ascii_lowercase(),
                    day(p.capture_time.as_deref()),
                );
                if let Some(m) = p.origin_modified.as_deref() {
                    b.push_str(&format!(" Server says it last changed {}.", day(Some(m))));
                }
                b
            }
            None => "REVENANT annotated view.".to_string(),
        };
        Self {
            page_path: cert.page.clone(),
            site: cert.site.clone(),
            files,
            restored,
            banner,
        }
    }
}

/// Dimension attributes become inline sizes only when they are plain digits (R8.6).
fn pixels(value: Option<String>) -> Option<u32> {
    let v = value?;
    let v = v.trim();
    if v.is_empty() || v.len() > 4 || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse().ok().filter(|n| *n > 0)
}

/// Annotate a restored page. Returns the input unchanged when there is nothing to annotate (R8.8).
pub fn annotate_html(input: &[u8], a: &Annotation) -> Result<Vec<u8>, RewriteError> {
    let text: String = input.iter().map(|&b| b as char).collect();
    let changed = RefCell::new(false);
    let hosts = [a.site.as_str()];

    let settings = RewriteStrSettings::new()
        .append_element_content_handler(element!("img", |el: &mut Element| {
            let Some(src) = el.get_attribute("src") else {
                return Ok(());
            };
            // External images, and anything that is not a site file, are left alone (R8.4).
            let Some(path) = site_path_of(&a.page_path, &src, &hosts) else {
                return Ok(());
            };
            let Some(note) = a.files.get(&path) else {
                return Ok(());
            };
            *changed.borrow_mut() = true;
            if a.restored.contains(&path) {
                el.before(
                    &format!(
                        "<span class=\"rv rv-{}\" data-rv-tag=\"{}\" title=\"{}\">",
                        note.class,
                        esc(&note.tag),
                        esc(&note.detail)
                    ),
                    ContentType::Html,
                );
                el.after("</span>", ContentType::Html);
            } else {
                let size = match (
                    pixels(el.get_attribute("width")),
                    pixels(el.get_attribute("height")),
                ) {
                    (Some(w), Some(h)) => format!(" style=\"width:{w}px;height:{h}px\""),
                    (Some(w), None) => format!(" style=\"width:{w}px\""),
                    (None, Some(h)) => format!(" style=\"height:{h}px\""),
                    (None, None) => String::new(),
                };
                el.replace(
                    &format!(
                        "<span class=\"rv-gap rv-{}\"{size} title=\"{}\">{}</span>",
                        note.class,
                        esc(&note.detail),
                        esc(&note.tag)
                    ),
                    ContentType::Html,
                );
            }
            Ok(())
        }))
        .append_element_content_handler(element!("body", |el: &mut Element| {
            *changed.borrow_mut() = true;
            el.prepend(
                &format!("{STYLE}<div id=\"rv-bar\">{}</div>", esc(&a.banner)),
                ContentType::Html,
            );
            Ok(())
        }));

    let out = rewrite_str(&text, settings).map_err(|e| RewriteError::Html(e.to_string()))?;
    if !*changed.borrow() {
        return Ok(input.to_vec());
    }
    let mut bytes = Vec::with_capacity(out.len());
    for ch in out.chars() {
        let code = ch as u32;
        if code > 0xFF {
            return Err(RewriteError::NonByteOutput);
        }
        bytes.push(code as u8);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(class: &'static str, tag: &str) -> FileNote {
        FileNote {
            class,
            tag: tag.to_string(),
            detail: format!("detail for {tag}"),
        }
    }

    fn annotation() -> Annotation {
        let mut files = BTreeMap::new();
        files.insert(
            "images/a.gif".to_string(),
            note("attested", "attested 1999-12-27"),
        );
        files.insert(
            "images/gone.gif".to_string(),
            note("missing", "not archived"),
        );
        let restored: BTreeSet<String> = ["images/a.gif".to_string()].into();
        Annotation {
            page_path: "index.html".into(),
            site: "y2k.gov".into(),
            files,
            restored,
            banner: "REVENANT annotated view. This page: attested.".into(),
        }
    }

    fn run(html: &str) -> String {
        let out = annotate_html(html.as_bytes(), &annotation()).expect("annotates");
        out.iter().map(|&b| b as char).collect()
    }

    #[test]
    fn a_restored_image_is_wrapped_and_tagged() {
        let out = run(r#"<body><img src="images/a.gif" width="130" height="40"></body>"#);
        assert!(out.contains(r#"<span class="rv rv-attested" data-rv-tag="attested 1999-12-27" title="detail for attested 1999-12-27"><img src="images/a.gif" width="130" height="40"></span>"#), "{out}");
    }

    #[test]
    fn a_missing_image_becomes_a_placeholder_of_the_same_size() {
        let out = run(r#"<body><img src="images/gone.gif" width="130" height="40"></body>"#);
        assert!(!out.contains("<img"), "{out}");
        assert!(out.contains(r#"<span class="rv-gap rv-missing" style="width:130px;height:40px" title="detail for not archived">not archived</span>"#), "{out}");
    }

    #[test]
    fn sizes_that_are_not_plain_digits_are_not_copied_into_markup() {
        for width in ["50%", "100px", "1e9", "\" onload=\"x", "99999", "0", ""] {
            let html = format!(r#"<body><img src="images/gone.gif" width="{width}"></body>"#);
            let out = run(&html);
            // The placeholder is there, and none of the page's width text became an inline size.
            assert!(
                out.contains(r#"<span class="rv-gap rv-missing" title="#),
                "{width:?}: {out}"
            );
            assert!(!out.contains("style=\"width"), "{width:?}: {out}");
        }
    }

    #[test]
    fn external_images_and_unknown_files_are_left_alone() {
        let out = run(
            r#"<body><img src="http://other.example/x.gif"><img src="images/never-heard-of.gif"></body>"#,
        );
        assert!(
            out.contains(r#"<img src="http://other.example/x.gif">"#),
            "{out}"
        );
        assert!(
            out.contains(r#"<img src="images/never-heard-of.gif">"#),
            "{out}"
        );
        assert!(!out.contains("<span class=\"rv-gap"), "{out}");
        assert!(!out.contains("<span class=\"rv rv-"), "{out}");
    }

    #[test]
    fn the_site_may_be_named_by_absolute_url() {
        let out = run(r#"<body><img src="http://www.y2k.gov:80/images/gone.gif"></body>"#);
        assert!(out.contains("rv-gap rv-missing"), "{out}");
    }

    #[test]
    fn the_bar_and_style_are_added_once_at_the_top_of_the_body() {
        let out = run("<html><body class=\"x\"><p>hi</p></body></html>");
        assert_eq!(out.matches("id=\"rv-bar\"").count(), 1, "{out}");
        assert_eq!(out.matches("<style>").count(), 1, "{out}");
        assert!(out.find("rv-bar").unwrap() < out.find("<p>hi</p>").unwrap());
        assert!(out.contains(r#"<body class="x"><style>"#), "{out}");
    }

    #[test]
    fn nothing_to_annotate_returns_the_input_unchanged() {
        let html = b"<p>no body and no images, just text \xe9</p>";
        assert_eq!(annotate_html(html, &annotation()).unwrap(), html);
    }

    #[test]
    fn non_ascii_bytes_outside_the_markup_survive() {
        let html = b"<body><p>caf\xe9</p></body>";
        let out = annotate_html(html, &annotation()).unwrap();
        let needle: &[u8] = b"<p>caf\xe9</p>";
        assert!(out.windows(needle.len()).any(|w| w == needle));
    }

    #[test]
    fn inserted_text_is_escaped_and_ascii_only() {
        assert_eq!(esc("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&#39;");
        // Each character outside printable ASCII becomes one question mark.
        assert_eq!(esc("caf\u{e9} x"), "caf? x");
        assert_eq!(esc("a\u{2014}b"), "a?b");
        assert_eq!(esc("tab\there"), "tab?here");
    }

    #[test]
    fn scripts_in_the_page_are_not_run_or_copied_into_inserted_markup() {
        // The rewriter has already removed scripts. The annotator must not add any, nor repeat
        // anything the page said.
        let out =
            run(r#"<body><img src="images/gone.gif" alt="<script>alert(1)</script>"></body>"#);
        assert!(!out.contains("<script"), "{out}");
        assert!(!out.contains("alert"), "{out}");
    }

    /// R8.10: a tag under an image must fit that image, so it is never cut off or wider than the
    /// image. The specimen's smallest images are 130 px wide. At 10 px monospace a character is
    /// about 6 px, and the tag has 6 px of padding. Placeholders are not covered: they are boxes of
    /// their own with a minimum width, and there is no image beneath them.
    #[test]
    fn every_tag_under_an_image_fits_a_130_pixel_image() {
        let widest = 130 - 6;
        let sample = |verdict: &'static str, included: bool| FileRecord {
            path: "x".into(),
            verdict,
            verdict_from_dates_only: verdict,
            basis: String::new(),
            included,
            capture_time: Some("20000304041110".into()),
            origin_modified: Some("19991227194103".into()),
            delta_days: None,
            archive_digest: None,
            sha256: None,
            bytes: None,
            note: None,
        };
        for verdict in ["PROVEN", "ATTESTED", "LIKELY", "UNCERTAIN", "ANACHRONISM"] {
            let tag = note_for(&sample(verdict, true)).tag;
            assert!(
                tag.len() * 6 <= widest,
                "{verdict}: {tag:?} is {} px",
                tag.len() * 6
            );
        }
        // Everything inserted is plain ASCII, whatever the verdict.
        for (verdict, included) in [
            ("ANACHRONISM", false),
            ("MISSING", false),
            ("UNKNOWN", false),
        ] {
            let n = note_for(&sample(verdict, included));
            assert!(n.tag.is_ascii() && n.detail.is_ascii());
        }
    }
}
