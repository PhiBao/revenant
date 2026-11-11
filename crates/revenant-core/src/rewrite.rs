//! HTML de-archiving and link repair.
//!
//! Turns a page captured from a web archive into a self-contained page:
//! - unwraps archive-rewritten URLs (`/web/<ts>/<original>`) back to the original URL,
//! - points links to restored files at their relative path inside the ZIP,
//! - flags links to files that were not restored (`data-revenant="not-restored"`),
//! - removes scripts, inline event handlers, `javascript:` links, `<base>` and comments.
//!
//! Bytes are preserved. The page is read as Latin-1 (one byte to one char), rewritten, and
//! written back the same way, so any ASCII-compatible encoding keeps its non-ASCII bytes. The
//! rewriter only ever inserts ASCII.
//!
//! Limits: text inside the page that mentions an archive host is left alone, and URLs inside
//! inline CSS or scripts are not rewritten.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

use lol_html::html_content::Element;
use lol_html::{doc_comments, element, rewrite_str, RewriteStrSettings};

const URL_ATTRS: [&str; 5] = ["src", "href", "background", "action", "poster"];
const ARCHIVE_HOST: &str = "web.archive.org";

/// What the rewriter needs to know about the page and the restored site.
pub struct Context<'a> {
    /// Path of this page inside the ZIP, for example `docs/a.html`.
    pub page_path: &'a str,
    /// Every path that is in the ZIP.
    pub restored: &'a BTreeSet<String>,
    /// Hosts that count as "this site", for example `y2k.gov`. A leading `www.` is ignored.
    pub site_hosts: &'a [&'a str],
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub bytes: Vec<u8>,
    /// Site paths the page links to that are not in the ZIP, in first-seen order, no duplicates.
    pub not_restored: Vec<String>,
    /// Embedded files the page needs (images, styles, backgrounds), as site paths.
    pub assets: Vec<String>,
    /// Navigation targets (`<a href>`, forms), as site paths. Never restored in single-page scope.
    pub links: Vec<String>,
    pub scripts_removed: usize,
    pub archive_urls_unwrapped: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RewriteError {
    /// The HTML rewriter rejected the input.
    Html(String),
    /// Output held a character that is not a single byte, which would corrupt the page.
    NonByteOutput,
}

/// Rewrite one archived page.
pub fn rewrite_html(input: &[u8], ctx: &Context) -> Result<Output, RewriteError> {
    let text: String = input.iter().map(|&b| b as char).collect();

    let not_restored = RefCell::new(Vec::<String>::new());
    let assets = RefCell::new(Vec::<String>::new());
    let links = RefCell::new(Vec::<String>::new());
    let scripts = Cell::new(0usize);
    let unwrapped = Cell::new(0usize);

    let settings = RewriteStrSettings::new()
        .append_element_content_handler(element!("script", |el: &mut Element| {
            scripts.set(scripts.get() + 1);
            el.remove();
            Ok(())
        }))
        .append_element_content_handler(element!("base", |el: &mut Element| {
            el.remove();
            Ok(())
        }))
        .append_element_content_handler(element!("*", |el: &mut Element| {
            // Inline event handlers run script, so they go with the scripts.
            let event_attrs: Vec<String> = el
                .attributes()
                .iter()
                .map(|a| a.name())
                .filter(|n| n.len() > 2 && n.starts_with("on"))
                .collect();
            for name in event_attrs {
                el.remove_attribute(&name);
            }

            let mut flagged = false;
            let tag = el.tag_name();
            for attr in URL_ATTRS {
                let Some(value) = el.get_attribute(attr) else {
                    continue;
                };
                let is_link = (attr == "href" && (tag == "a" || tag == "area")) || attr == "action";
                let res = resolve(ctx, &value);
                let seen = match &res {
                    Resolution::Replace {
                        site_path: Some(p), ..
                    } => Some(p.clone()),
                    Resolution::NotRestored { path, .. } => Some(path.clone()),
                    _ => None,
                };
                if let Some(p) = seen {
                    let list = if is_link { &links } else { &assets };
                    let mut list = list.borrow_mut();
                    if !list.contains(&p) {
                        list.push(p);
                    }
                }
                match res {
                    Resolution::Unchanged => {}
                    Resolution::Replace {
                        value, was_archive, ..
                    } => {
                        if was_archive {
                            unwrapped.set(unwrapped.get() + 1);
                        }
                        // Values are ASCII or unchanged bytes; setting cannot fail on valid names.
                        let _ = el.set_attribute(attr, &value);
                    }
                    Resolution::NotRestored {
                        value,
                        path,
                        was_archive,
                    } => {
                        if was_archive {
                            unwrapped.set(unwrapped.get() + 1);
                        }
                        let _ = el.set_attribute(attr, &value);
                        let mut list = not_restored.borrow_mut();
                        if !list.contains(&path) {
                            list.push(path);
                        }
                        flagged = true;
                    }
                    Resolution::Drop => {
                        let _ = el.set_attribute(attr, "#");
                    }
                }
            }
            if flagged {
                let _ = el.set_attribute("data-revenant", "not-restored");
            }
            Ok(())
        }))
        .append_document_content_handler(doc_comments!(|c| {
            c.remove();
            Ok(())
        }));

    let out = rewrite_str(&text, settings).map_err(|e| RewriteError::Html(e.to_string()))?;

    let mut bytes = Vec::with_capacity(out.len());
    for ch in out.chars() {
        let code = ch as u32;
        if code > 0xFF {
            return Err(RewriteError::NonByteOutput);
        }
        bytes.push(code as u8);
    }

    Ok(Output {
        bytes,
        not_restored: not_restored.into_inner(),
        assets: assets.into_inner(),
        links: links.into_inner(),
        scripts_removed: scripts.get(),
        archive_urls_unwrapped: unwrapped.get(),
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Resolution {
    Unchanged,
    Replace {
        value: String,
        was_archive: bool,
        /// Site-root path when the value points at this site.
        site_path: Option<String>,
    },
    NotRestored {
        value: String,
        path: String,
        was_archive: bool,
    },
    Drop,
}

fn resolve(ctx: &Context, raw: &str) -> Resolution {
    let value = raw.trim();
    let lower = value.to_ascii_lowercase();
    if value.is_empty()
        || value.starts_with('#')
        || lower.starts_with("mailto:")
        || lower.starts_with("data:")
    {
        return Resolution::Unchanged;
    }
    if lower.starts_with("javascript:") {
        return Resolution::Drop;
    }

    let (unwrapped, was_archive) = match unwrap_archive(value) {
        Some(original) => (original, true),
        None => (value.to_string(), false),
    };

    // Absolute URL: decide whether it is this site, another site, or the archive itself.
    let site_path = if let Some((host, rest)) = split_absolute(&unwrapped) {
        if host == ARCHIVE_HOST {
            return Resolution::Drop;
        }
        if is_site_host(ctx, &host) {
            rest
        } else if was_archive {
            return Resolution::Replace {
                value: unwrapped,
                was_archive,
                site_path: None,
            };
        } else {
            return Resolution::Unchanged;
        }
    } else {
        unwrapped.clone()
    };

    // Split off the query and fragment. The query is dropped, the fragment is kept.
    let (without_frag, frag) = match site_path.split_once('#') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (site_path.clone(), None),
    };
    let path_part = without_frag.split('?').next().unwrap_or("").to_string();
    if path_part.is_empty() {
        return Resolution::Unchanged;
    }

    let Some(target) = normalize(ctx.page_path, &path_part) else {
        return Resolution::NotRestored {
            value: unwrapped,
            path: path_part,
            was_archive,
        };
    };

    if ctx.restored.contains(&target) {
        let mut rel = relative_to(ctx.page_path, &target);
        if let Some(f) = frag {
            rel.push('#');
            rel.push_str(&f);
        }
        Resolution::Replace {
            value: rel,
            was_archive,
            site_path: Some(target),
        }
    } else {
        Resolution::NotRestored {
            value: unwrapped,
            path: target,
            was_archive,
        }
    }
}

/// `https://web.archive.org/web/20000101000000im_/http://x/y` becomes `http://x/y`.
/// Also accepts the host-less form `/web/20000101000000/...`.
fn unwrap_archive(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    let after_prefix = [
        "https://web.archive.org/web/",
        "http://web.archive.org/web/",
        "//web.archive.org/web/",
    ]
    .iter()
    .find(|p| lower.starts_with(*p))
    .map(|p| &value[p.len()..])
    .or_else(|| {
        let rest = value.strip_prefix("/web/")?;
        rest.chars().next().filter(char::is_ascii_digit)?;
        Some(rest)
    })?;
    let (stamp, original) = after_prefix.split_once('/')?;
    let valid = !stamp.is_empty() && stamp.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return None;
    }
    // Some archives collapse `http://` to `http:/`.
    let original = original
        .strip_prefix("http:/")
        .filter(|r| !r.starts_with('/'))
        .map(|r| format!("http://{r}"))
        .or_else(|| {
            original
                .strip_prefix("https:/")
                .filter(|r| !r.starts_with('/'))
                .map(|r| format!("https://{r}"))
        })
        .unwrap_or_else(|| original.to_string());
    Some(original)
}

/// Split `http://host:80/path?q` into (`host`, `/path?q`). Returns `None` for relative values.
fn split_absolute(value: &str) -> Option<(String, String)> {
    let lower = value.to_ascii_lowercase();
    let rest = if lower.starts_with("http://") || lower.starts_with("https://") {
        &value[value.find("//")? + 2..]
    } else {
        value.strip_prefix("//")?
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, h)| h)
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let path = if end < rest.len() {
        let tail = &rest[end..];
        if tail.starts_with('/') {
            tail.to_string()
        } else {
            format!("/{tail}")
        }
    } else {
        "/".to_string()
    };
    Some((host, path))
}

fn is_site_host(ctx: &Context, host: &str) -> bool {
    let bare = host.strip_prefix("www.").unwrap_or(host);
    ctx.site_hosts.iter().any(|h| {
        h.strip_prefix("www.")
            .unwrap_or(h)
            .eq_ignore_ascii_case(bare)
    })
}

/// Resolve `reference` against the directory of `page_path`. Returns the site-root path, or
/// `None` when the reference climbs above the site root.
fn normalize(page_path: &str, reference: &str) -> Option<String> {
    let mut stack: Vec<&str> = Vec::new();
    if !reference.starts_with('/') {
        let mut dir: Vec<&str> = page_path.split('/').collect();
        dir.pop();
        stack.extend(dir);
    }
    for seg in reference.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop()?;
            }
            s => stack.push(s),
        }
    }
    if stack.is_empty() {
        return None;
    }
    Some(stack.join("/"))
}

/// Relative path from the directory of `page_path` to `target`.
fn relative_to(page_path: &str, target: &str) -> String {
    let mut from: Vec<&str> = page_path.split('/').collect();
    from.pop();
    let to: Vec<&str> = target.split('/').collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; from.len() - common];
    parts.extend(&to[common..]);
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restored(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|p| (*p).to_string()).collect()
    }

    fn run(page: &str, set: &[&str], html: &str) -> Output {
        let r = restored(set);
        let ctx = Context {
            page_path: page,
            restored: &r,
            site_hosts: &["y2k.gov"],
        };
        rewrite_html(html.as_bytes(), &ctx).expect("rewrites")
    }

    fn text(o: &Output) -> String {
        o.bytes.iter().map(|&b| b as char).collect()
    }

    #[test]
    fn keeps_a_restored_relative_link() {
        let o = run("a.html", &["images/1.gif"], r#"<img src="images/1.gif">"#);
        assert_eq!(text(&o), r#"<img src="images/1.gif">"#);
        assert!(o.not_restored.is_empty());
    }

    #[test]
    fn flags_a_link_to_a_file_that_was_not_restored() {
        let o = run("a.html", &[], r#"<a href="docs/x.html">x</a>"#);
        assert!(text(&o).contains(r#"data-revenant="not-restored""#));
        assert_eq!(o.not_restored, ["docs/x.html"]);
    }

    #[test]
    fn unwraps_archive_urls_to_local_paths() {
        let o = run(
            "a.html",
            &["images/1.gif"],
            r#"<img src="https://web.archive.org/web/20000304045622im_/http://www.y2k.gov:80/images/1.gif">"#,
        );
        assert_eq!(text(&o), r#"<img src="images/1.gif">"#);
        assert_eq!(o.archive_urls_unwrapped, 1);
    }

    #[test]
    fn unwraps_host_less_archive_paths() {
        let o = run(
            "a.html",
            &["images/1.gif"],
            r#"<img src="/web/20000304045622im_/http://y2k.gov/images/1.gif">"#,
        );
        assert_eq!(text(&o), r#"<img src="images/1.gif">"#);
    }

    #[test]
    fn external_links_lose_the_archive_wrapper_only() {
        let o = run(
            "a.html",
            &[],
            r#"<a href="http://web.archive.org/web/2000/http://www.latimes.com/x">l</a>"#,
        );
        assert_eq!(text(&o), r#"<a href="http://www.latimes.com/x">l</a>"#);
    }

    #[test]
    fn plain_external_links_are_untouched() {
        let o = run(
            "a.html",
            &[],
            r#"<a href="http://www.ky3.com/bytes/">k</a>"#,
        );
        assert_eq!(text(&o), r#"<a href="http://www.ky3.com/bytes/">k</a>"#);
    }

    #[test]
    fn links_from_a_nested_page_are_relative_to_that_page() {
        let o = run(
            "docs/a.html",
            &["images/1.gif"],
            r#"<img src="/images/1.gif"><img src="../images/1.gif">"#,
        );
        assert_eq!(
            text(&o),
            r#"<img src="../images/1.gif"><img src="../images/1.gif">"#
        );
    }

    #[test]
    fn removes_scripts_handlers_javascript_links_base_and_comments() {
        let o = run(
            "a.html",
            &[],
            r#"<base href="http://x/"><script>alert(1)</script><a href="javascript:x()" onclick="y()">a</a><!-- note -->"#,
        );
        let t = text(&o);
        assert!(!t.contains("script"), "{t}");
        assert!(!t.contains("onclick"), "{t}");
        assert!(!t.contains("base"), "{t}");
        assert!(!t.contains("note"), "{t}");
        assert!(t.contains(r##"href="#""##), "{t}");
        assert_eq!(o.scripts_removed, 1);
    }

    #[test]
    fn non_ascii_bytes_survive_unchanged() {
        let r = restored(&[]);
        let ctx = Context {
            page_path: "a.html",
            restored: &r,
            site_hosts: &["y2k.gov"],
        };
        let input: &[u8] = b"<p>caf\xe9 \xe2\x80\x94 ok</p>";
        let out = rewrite_html(input, &ctx).expect("rewrites");
        assert_eq!(out.bytes, input);
    }

    #[test]
    fn fragments_are_kept_and_queries_dropped() {
        let o = run("a.html", &["b.html"], r##"<a href="b.html?x=1#top">b</a>"##);
        assert_eq!(text(&o), r##"<a href="b.html#top">b</a>"##);
    }

    #[test]
    fn climbing_above_the_site_root_is_not_restored() {
        let o = run("a.html", &[], r#"<a href="../../etc/passwd">x</a>"#);
        assert!(text(&o).contains("not-restored"));
    }
}
