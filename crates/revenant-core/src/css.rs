//! Reading and rewriting references in CSS (requirement R9).
//!
//! Finds `url(...)` and `@import` targets in a stylesheet, a `<style>` block or a `style=""`
//! attribute, and treats each one like an HTML reference: archive wrappers are removed, references
//! to restored files become relative, references to other site files are left as they were and
//! reported. Constructs that run script in old browsers are neutralised.
//!
//! As in the HTML rewriter, text is read one byte to one character, so any ASCII-compatible
//! encoding keeps its non-ASCII bytes. Nothing in this module can fail: unreadable input is copied
//! through unchanged.

use crate::rewrite::{resolve, Context, Resolution};

/// What a pass over some CSS found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CssFindings {
    /// Site files the CSS refers to, in first-seen order, no duplicates.
    pub assets: Vec<String>,
    /// The subset of `assets` that is pulled in with `@import`. An import is always a stylesheet,
    /// whatever its file name looks like.
    pub imports: Vec<String>,
    /// Site files the CSS refers to that are not in the ZIP.
    pub not_restored: Vec<String>,
    /// Script-running constructs that were neutralised (R9.5).
    pub neutralised: usize,
    pub archive_urls_unwrapped: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CssOutput {
    pub bytes: Vec<u8>,
    pub findings: CssFindings,
}

/// Rewrite a stylesheet (R9.1). `ctx.page_path` is the stylesheet's own path.
pub fn rewrite_css(input: &[u8], ctx: &Context) -> CssOutput {
    let text: String = input.iter().map(|&b| b as char).collect();
    let (out, findings) = rewrite_css_str(&text, ctx);
    // Every character came from a byte or is ASCII, so this cannot lose information.
    let bytes = out.chars().map(|c| c as u32 as u8).collect();
    CssOutput { bytes, findings }
}

/// Rewrite CSS that is already one-character-per-byte text, as `<style>` content is.
pub fn rewrite_css_str(text: &str, ctx: &Context) -> (String, CssFindings) {
    let chars: Vec<char> = text.chars().collect();
    let mut s = Scanner {
        chars: &chars,
        ctx,
        out: String::with_capacity(text.len()),
        findings: CssFindings::default(),
    };
    s.run();
    (s.out, s.findings)
}

struct Scanner<'a> {
    chars: &'a [char],
    ctx: &'a Context<'a>,
    out: String,
    findings: CssFindings,
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

impl Scanner<'_> {
    fn at(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    /// True when the text at `i` equals `word`, ignoring ASCII case.
    fn matches(&self, i: usize, word: &str) -> bool {
        word.chars()
            .enumerate()
            .all(|(k, w)| self.at(i + k).is_some_and(|c| c.eq_ignore_ascii_case(&w)))
    }

    fn copy(&mut self, from: usize, to: usize) {
        for c in &self.chars[from..to.min(self.chars.len())] {
            self.out.push(*c);
        }
    }

    fn run(&mut self) {
        let n = self.chars.len();
        let mut i = 0;
        // True between `@import` and the thing it imports.
        let mut import_pending = false;

        while i < n {
            let c = self.chars[i];

            // Comments: copied through, never read (R9.3).
            if c == '/' && self.at(i + 1) == Some('*') {
                let end = self.find_comment_end(i + 2);
                self.copy(i, end);
                i = end;
                continue;
            }

            // Strings: a URL only after @import; otherwise copied through (R9.3).
            if c == '"' || c == '\'' {
                let end = self.string_end(i);
                if import_pending && end > i + 1 && self.at(end - 1) == Some(c) {
                    let inner: String = self.chars[i + 1..end - 1].iter().collect();
                    let value = self.reference(&inner, true);
                    self.out.push(c);
                    self.out.push_str(&value);
                    self.out.push(c);
                } else {
                    self.copy(i, end);
                }
                import_pending = false;
                i = end;
                continue;
            }

            if c.is_whitespace() {
                self.out.push(c);
                i += 1;
                continue;
            }

            let prev_ident = i > 0 && is_ident(self.chars[i - 1]);

            if c == '@' && self.matches(i, "@import") && !self.at(i + 7).is_some_and(is_ident) {
                self.copy(i, i + 7);
                i += 7;
                import_pending = true;
                continue;
            }

            if !prev_ident && self.matches(i, "url(") {
                if let Some(next) = self.url_token(i, import_pending) {
                    i = next;
                    import_pending = false;
                    continue;
                }
            }

            if !prev_ident && self.matches(i, "expression") {
                let mut j = i + "expression".len();
                while self.at(j).is_some_and(char::is_whitespace) {
                    j += 1;
                }
                if self.at(j) == Some('(') {
                    self.out.push_str("x-removed");
                    self.findings.neutralised += 1;
                    i += "expression".len();
                    import_pending = false;
                    continue;
                }
            }

            if !prev_ident && self.matches(i, "behavior") {
                let mut j = i + "behavior".len();
                while self.at(j).is_some_and(char::is_whitespace) {
                    j += 1;
                }
                if self.at(j) == Some(':') {
                    self.out.push_str("x-removed");
                    self.findings.neutralised += 1;
                    i += "behavior".len();
                    import_pending = false;
                    continue;
                }
            }

            if self.matches(i, "-moz-binding") {
                self.out.push_str("x-removed");
                self.findings.neutralised += 1;
                i += "-moz-binding".len();
                import_pending = false;
                continue;
            }

            self.out.push(c);
            import_pending = false;
            i += 1;
        }
    }

    fn find_comment_end(&self, from: usize) -> usize {
        let mut j = from;
        while j < self.chars.len() {
            if self.chars[j] == '*' && self.at(j + 1) == Some('/') {
                return j + 2;
            }
            j += 1;
        }
        self.chars.len()
    }

    /// Index just past the closing quote, or the end of the text when the string never closes.
    /// A backslash escapes the next character.
    fn string_end(&self, start: usize) -> usize {
        let quote = self.chars[start];
        let mut j = start + 1;
        while j < self.chars.len() {
            match self.chars[j] {
                '\\' => j += 2,
                c if c == quote => return j + 1,
                // A raw newline ends a string in CSS.
                '\n' => return j,
                _ => j += 1,
            }
        }
        self.chars.len()
    }

    /// Handles `url(...)` starting at `start`. Returns the index after it, or `None` when it is
    /// malformed, in which case the caller copies the characters through one by one.
    ///
    /// Only the reference itself is replaced. Whitespace inside the parentheses, and the quotes,
    /// are copied exactly as written (R9.9).
    fn url_token(&mut self, start: usize, is_import: bool) -> Option<usize> {
        let open = start + 4;
        let mut j = open;
        while self.at(j).is_some_and(char::is_whitespace) {
            j += 1;
        }
        let lead_end = j;
        let (raw, content_end, quote) = match self.at(j)? {
            q @ ('"' | '\'') => {
                let end = self.string_end(j);
                if self.at(end.checked_sub(1)?) != Some(q) || end <= j + 1 {
                    return None;
                }
                let inner: String = self.chars[j + 1..end - 1].iter().collect();
                (inner, end, Some(q))
            }
            _ => {
                let mut k = j;
                while let Some(c) = self.at(k) {
                    if c == ')' {
                        break;
                    }
                    if c == '"' || c == '\'' || c == '(' || c == '\n' {
                        return None;
                    }
                    k += 1;
                }
                self.at(k)?;
                // Trailing whitespace before `)` is not part of an unquoted reference.
                let mut end = k;
                while end > j && self.chars[end - 1].is_whitespace() {
                    end -= 1;
                }
                let inner: String = self.chars[j..end].iter().collect();
                (inner, end, None)
            }
        };
        let mut close = content_end;
        while self.at(close).is_some_and(char::is_whitespace) {
            close += 1;
        }
        if self.at(close) != Some(')') {
            return None;
        }

        let value = self.reference(&raw, is_import);
        self.copy(start, lead_end); // "url(" and any whitespace
        if let Some(q) = quote {
            self.out.push(q);
            self.out.push_str(&value);
            self.out.push(q);
        } else {
            self.out.push_str(&value);
        }
        self.copy(content_end, close + 1); // whitespace and the closing parenthesis
        Some(close + 1)
    }

    /// Resolve one reference the way the HTML rewriter does, and record what it was.
    fn reference(&mut self, raw: &str, is_import: bool) -> String {
        match resolve(self.ctx, raw) {
            Resolution::Unchanged => raw.to_string(),
            Resolution::Drop => "#".to_string(),
            Resolution::Replace {
                value,
                was_archive,
                site_path,
            } => {
                if was_archive {
                    self.findings.archive_urls_unwrapped += 1;
                }
                if let Some(p) = site_path {
                    if is_import {
                        push_unique(&mut self.findings.imports, p.clone());
                    }
                    push_unique(&mut self.findings.assets, p);
                }
                safe_or(raw, value)
            }
            Resolution::NotRestored {
                value,
                path,
                was_archive,
            } => {
                if was_archive {
                    self.findings.archive_urls_unwrapped += 1;
                }
                if is_import {
                    push_unique(&mut self.findings.imports, path.clone());
                }
                push_unique(&mut self.findings.assets, path.clone());
                push_unique(&mut self.findings.not_restored, path);
                safe_or(raw, value)
            }
        }
    }
}

/// A rewritten value is only used when it cannot end the token it sits in. Otherwise the original
/// text is kept, so a hostile reference can never break out of its `url(...)`.
fn safe_or(raw: &str, value: String) -> String {
    let unsafe_char =
        |c: char| matches!(c, '"' | '\'' | '(' | ')' | '\\' | '\n' | '\r') || c.is_whitespace();
    if value.chars().any(unsafe_char) {
        raw.to_string()
    } else {
        value
    }
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn ctx<'a>(restored: &'a BTreeSet<String>) -> Context<'a> {
        Context {
            page_path: "css/site.css",
            restored,
            site_hosts: &["y2k.gov"],
        }
    }

    fn run(css: &str, restored: &[&str]) -> (String, CssFindings) {
        let set: BTreeSet<String> = restored.iter().map(|s| (*s).to_string()).collect();
        let out = rewrite_css(css.as_bytes(), &ctx(&set));
        (out.bytes.iter().map(|&b| b as char).collect(), out.findings)
    }

    #[test]
    fn finds_url_references_quoted_and_unquoted_and_resolves_them_against_the_stylesheet() {
        let css = "a{background:url(../img/a.gif)} b{background:url('../img/b.gif')} c{background:url( \"../img/c.gif\" )}";
        let (out, f) = run(css, &["img/a.gif", "img/b.gif", "img/c.gif"]);
        assert_eq!(f.assets, ["img/a.gif", "img/b.gif", "img/c.gif"]);
        assert_eq!(
            out, css,
            "already-relative references to restored files stay as they are"
        );
        assert!(f.not_restored.is_empty());
    }

    #[test]
    fn finds_both_import_forms() {
        let (_, f) = run(
            "@import \"base.css\"; @import url(print.css); @import 'x/y.css' screen;",
            &[],
        );
        assert_eq!(f.assets, ["css/base.css", "css/print.css", "css/x/y.css"]);
    }

    #[test]
    fn a_reference_to_a_missing_site_file_is_kept_and_reported() {
        let (out, f) = run("a{background:url(gone.gif)}", &[]);
        assert_eq!(out, "a{background:url(gone.gif)}");
        assert_eq!(f.not_restored, ["css/gone.gif"]);
    }

    #[test]
    fn absolute_and_archive_wrapped_urls_become_relative_paths() {
        let (out, f) = run(
            "a{background:url(http://www.y2k.gov:80/img/a.gif)} b{background:url(https://web.archive.org/web/20000101im_/http://y2k.gov/img/b.gif)}",
            &["img/a.gif", "img/b.gif"],
        );
        assert_eq!(
            out,
            "a{background:url(../img/a.gif)} b{background:url(../img/b.gif)}"
        );
        assert_eq!(f.archive_urls_unwrapped, 1);
    }

    #[test]
    fn external_data_and_javascript_urls() {
        let (out, f) = run(
            "a{background:url(http://other.example/x.gif)} b{background:url(data:image/gif;base64,R0lGOD)} c{background:url(javascript:alert(1))}",
            &[],
        );
        assert!(out.contains("url(http://other.example/x.gif)"), "{out}");
        assert!(out.contains("url(data:image/gif;base64,R0lGOD)"), "{out}");
        assert!(f.assets.is_empty(), "{:?}", f.assets);
    }

    #[test]
    fn comments_and_ordinary_strings_are_ignored() {
        let css = "/* url(commented.gif) @import 'c.css' */ a{content:\"url(x.gif)\"; font-family:'@import'}";
        let (out, f) = run(css, &[]);
        assert_eq!(out, css);
        assert!(f.assets.is_empty());
    }

    #[test]
    fn script_running_constructs_are_neutralised_and_counted() {
        let (out, f) = run(
            "a{width:expression(alert(1)); behavior:url(x.htc); -moz-binding:url(y.xml)}",
            &[],
        );
        assert!(!out.contains("expression"), "{out}");
        assert!(!out.contains("behavior"), "{out}");
        assert!(!out.contains("-moz-binding"), "{out}");
        assert_eq!(f.neutralised, 3);
        // The neutralised declarations no longer fetch anything on their own, but their urls are
        // still reported so a reader can see what they pointed at.
        assert!(f.assets.contains(&"css/x.htc".to_string()));
    }

    #[test]
    fn words_that_merely_contain_these_names_are_left_alone() {
        let css = "a{xexpression:1; mybehavior:2; background-url:3} .url(x){}";
        let (out, f) = run(css, &[]);
        assert_eq!(out, css);
        assert_eq!(f.neutralised, 0);
    }

    #[test]
    fn malformed_url_tokens_are_copied_not_guessed_at() {
        for css in [
            "a{background:url(",
            "a{background:url('x.gif}",
            "a{background:url(x.gif",
            "a{background:url(a b c)}",
            "url()",
        ] {
            let (out, _) = run(css, &["x.gif"]);
            assert_eq!(out, css, "{css:?}");
        }
    }

    #[test]
    fn a_rewritten_value_can_never_break_out_of_its_token() {
        // A fragment with a quote would end the string early if it were copied into the output.
        let (out, _) = run("a{background:url(\"b.gif#x\")}", &["css/b.gif"]);
        assert_eq!(out, "a{background:url(\"b.gif#x\")}");
        assert_eq!(safe_or("raw", "has space".into()), "raw");
        assert_eq!(safe_or("raw", "a)b".into()), "raw");
        assert_eq!(safe_or("raw", "a'b".into()), "raw");
        assert_eq!(safe_or("raw", "fine.gif".into()), "fine.gif");
    }

    #[test]
    fn non_ascii_bytes_survive() {
        let input: &[u8] = b"a{font-family:'caf\xe9'}\n/* \xff */";
        let set = BTreeSet::new();
        assert_eq!(rewrite_css(input, &ctx(&set)).bytes, input);
    }

    #[test]
    fn rewriting_twice_gives_the_same_bytes() {
        let css = "@import 'a.css'; a{background:url(http://y2k.gov/img/a.gif)} b{background:url(../img/gone.gif)}";
        let set: BTreeSet<String> = ["css/a.css".into(), "img/a.gif".into()].into();
        let once = rewrite_css(css.as_bytes(), &ctx(&set));
        let twice = rewrite_css(&once.bytes, &ctx(&set));
        assert_eq!(once.bytes, twice.bytes);
    }
}
