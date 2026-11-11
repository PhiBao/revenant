"""Build the three README diagrams as SVG. Run: python3 docs/diagrams/build.py

Each diagram answers one question:
  architecture   what talks to what, and what is real versus stored
  pipeline       how one archived page becomes a restored site and a certificate
  trust-boundary where untrusted input and untrusted content are contained
Palette matches the product: bone and dust for the archive, green only where something is safe or
true, rust where input or content cannot be trusted.
"""

from pathlib import Path
from xml.sax.saxutils import escape

INK, SLAB, RULE, RULE2 = "#0b0c0a", "#11130f", "#2a2c25", "#3c3f35"
BONE, DUST, FAINT = "#e8e3d2", "#a49d87", "#6f6a58"
LIFE, WRONG, DOUBT = "#7ee0a1", "#e0805a", "#dcb55b"
SERIF = "Georgia, 'Iowan Old Style', serif"
MONO = "ui-monospace, 'IBM Plex Mono', Menlo, Consolas, monospace"

TONES = {
    "plain": (SLAB, RULE2, BONE),
    "life": (SLAB, LIFE, BONE),
    "wrong": (SLAB, WRONG, BONE),
    "doubt": (SLAB, DOUBT, BONE),
    "quiet": (INK, RULE, DUST),
}


class Svg:
    def __init__(self, w, h, title, desc):
        self.w, self.h = w, h
        self.title, self.desc = title, desc
        self.parts = []

    def box(self, x, y, w, h, title, lines=(), tone="plain", mono_title=False, dashed=False):
        fill, stroke, text = TONES[tone]
        dash = ' stroke-dasharray="5 4"' if dashed else ""
        self.parts.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}" stroke="{stroke}" stroke-width="1.5"{dash}/>'
        )
        font = MONO if mono_title else SERIF
        size = 14 if mono_title else 17
        self.parts.append(
            f'<text x="{x + 14}" y="{y + 26}" font-family="{font}" font-size="{size}" fill="{text}">{escape(title)}</text>'
        )
        for i, line in enumerate(lines):
            self.parts.append(
                f'<text x="{x + 14}" y="{y + 48 + i * 17}" font-family="{MONO}" font-size="11.5" fill="{DUST}">{escape(line)}</text>'
            )

    def label(self, x, y, text, color=DUST, size=11.5, anchor="start", mono=True, italic=False):
        style = ' font-style="italic"' if italic else ""
        self.parts.append(
            f'<text x="{x}" y="{y}" font-family="{MONO if mono else SERIF}" font-size="{size}" fill="{color}" text-anchor="{anchor}"{style}>{escape(text)}</text>'
        )

    def arrow(self, x1, y1, x2, y2, color=DUST, dashed=False, label=None, lx=None, ly=None):
        dash = ' stroke-dasharray="5 4"' if dashed else ""
        mid = f"arrow-{color.lstrip('#')}"
        self.parts.append(
            f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="1.6"{dash} marker-end="url(#{mid})"/>'
        )
        if label:
            self.label(lx if lx is not None else (x1 + x2) / 2, ly if ly is not None else (y1 + y2) / 2 - 7, label, color, anchor="middle")

    def zone(self, x, y, w, h, name, color=RULE2):
        self.parts.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="{color}" stroke-width="1" stroke-dasharray="2 5"/>'
        )
        self.label(x + 12, y + 18, name.upper(), color, size=10.5)

    def render(self):
        colors = {DUST, LIFE, WRONG, DOUBT, BONE}
        defs = "".join(
            f'<marker id="arrow-{c.lstrip("#")}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="{c}"/></marker>'
            for c in colors
        )
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {self.w} {self.h}" width="{self.w}" height="{self.h}" role="img" aria-labelledby="t d">'
            f'<title id="t">{escape(self.title)}</title><desc id="d">{escape(self.desc)}</desc>'
            f"<defs>{defs}</defs>"
            f'<rect width="{self.w}" height="{self.h}" fill="{INK}"/>'
            + "".join(self.parts)
            + "</svg>\n"
        )


def architecture():
    s = Svg(
        980, 470,
        "Revenant architecture",
        "The browser talks to one Rust server. The server runs the engine, which reads captures through one interface that is either stored fixtures or the live Internet Archive.",
    )
    s.label(30, 34, "WHAT TALKS TO WHAT", FAINT, size=11)

    s.box(30, 70, 200, 110, "Browser", ["Next.js, built as static", "files. One column, four", "acts."], "plain")
    s.box(290, 70, 250, 110, "revenant-api", ["Axum. Jobs, ZIP download,", "live runs, rate limits.", "Also serves the web app."], "plain", mono_title=True)
    s.box(290, 250, 250, 110, "revenant-core", ["Pure Rust. No network, no", "framework. Classifier,", "rewriter, ZIP, pipeline."], "life", mono_title=True)
    s.box(600, 250, 215, 62, "SnapshotSource", ["one interface, two sources"], "quiet", mono_title=True)
    s.box(580, 372, 180, 84, "SeededSource", ["saved archive data in", "fixtures/snapshots/"], "plain", mono_title=True)
    s.box(780, 372, 180, 84, "LiveSource", ["asks the archive,", "one request at a time"], "plain", mono_title=True)
    s.box(760, 70, 200, 110, "Internet Archive", ["web.archive.org: the only", "host Revenant ever", "contacts."], "wrong")

    s.arrow(230, 110, 290, 110, DUST, label="JSON", lx=260, ly=100)
    s.arrow(290, 150, 230, 150, DUST)
    s.arrow(415, 180, 415, 250, DUST, label="resurrect()", lx=415, ly=222)
    s.arrow(540, 285, 600, 276, DUST)
    s.arrow(670, 312, 670, 372, DUST)
    s.arrow(740, 312, 840, 372, DUST)
    s.arrow(930, 372, 930, 180, WRONG)
    s.label(918, 280, "index + replay", WRONG, anchor="end")

    s.box(30, 250, 200, 110, "/p/ preview", ["Restored pages, served", "with a sandboxing CSP.", "Never from /api."], "life", mono_title=True)
    s.arrow(330, 180, 230, 262, LIFE, dashed=True)
    s.arrow(110, 250, 110, 180, LIFE, dashed=True)
    s.label(124, 222, "iframe", LIFE)

    s.label(30, 458, "Green = contained or verified. Rust = outside Revenant's control.", FAINT, size=11)
    return s.render()


def pipeline():
    s = Svg(
        980, 520,
        "The restore pipeline",
        "One archived page becomes a ZIP and a certificate in eight steps: look up the page, classify it, discover its assets, classify and fetch each, rewrite the page, write the certificate, pack a deterministic ZIP, hash it.",
    )
    s.label(30, 34, "HOW A CORPSE BECOMES A SITE", FAINT, size=11)

    steps = [
        ("1  Ask for the page", ["last capture on or before", "the date; first one after"]),
        ("2  Classify it", ["Proven, Likely, Uncertain,", "Anachronism or Missing"]),
        ("3  Fetch its body", ["the chosen capture, raw", "replay: no archive banner"]),
        ("4  Find the assets", ["read the page itself:", "images, styles, backgrounds"]),
    ]
    x = 18
    for i, (t, ls) in enumerate(steps):
        s.box(x, 70, 218, 92, t, ls, "plain")
        if i < len(steps) - 1:
            s.arrow(x + 218, 116, x + 246, 116, DUST)
        x += 246

    s.box(30, 220, 440, 130, "5  For each asset", [
        "ask, classify, then decide:",
        "Proven / Likely / Uncertain  ->  include",
        "Anachronism  ->  include only if you allow it",
        "Missing / Unknown  ->  leave out, keep in the ledger",
    ], "doubt")
    s.arrow(865, 162, 865, 190, DUST)
    s.arrow(865, 190, 250, 190, DUST)
    s.arrow(250, 190, 250, 220, DUST)

    s.box(520, 220, 430, 130, "6  Rewrite the page", [
        "unwrap archive URLs, point links at files in the ZIP",
        "flag links to files that were not restored",
        "remove scripts, inline handlers, base, comments",
        "bytes outside the markup are left exactly as they were",
    ], "life")
    s.arrow(470, 285, 520, 285, DUST)

    s.box(30, 400, 300, 92, "7  Write the certificate", ["every file: verdict, capture time,", "days from the date, SHA-256"], "plain")
    s.box(370, 400, 270, 92, "8  Pack and hash", ["entries sorted, times fixed,", "stored not compressed"], "plain")
    s.box(680, 400, 270, 92, "Same input, same bytes", ["the ZIP's SHA-256 is the", "same on every machine"], "life")
    s.arrow(735, 350, 735, 375, DUST)
    s.arrow(735, 375, 180, 375, DUST)
    s.arrow(180, 375, 180, 400, DUST)
    s.arrow(330, 446, 370, 446, DUST)
    s.arrow(640, 446, 680, 446, DUST)
    return s.render()


def trust():
    s = Svg(
        980, 520,
        "Trust boundaries",
        "Two kinds of untrusted data are contained. What the visitor types is validated and only sent as a query to the archive's index. What the archive returns is rewritten, stripped of script, and shown only inside a sandboxed frame.",
    )
    s.label(30, 34, "WHERE UNTRUSTED THINGS ARE CONTAINED", FAINT, size=11)
    s.label(950, 34, "dashed = a query going out   solid = bytes coming back", FAINT, size=10.5, anchor="end")

    s.zone(20, 56, 270, 440, "The visitor", WRONG)
    s.box(40, 90, 230, 92, "The address and date", ["typed by anyone"], "wrong")
    s.box(40, 210, 230, 110, "Never fetched", ["The address is not requested.", "Its host and path become a", "percent-encoded query."], "life")
    s.box(40, 350, 230, 120, "Rejected up front", ["IP addresses, localhost,", "user info, non-http schemes,", "bad paths, over 2048 bytes."], "life")

    s.zone(310, 56, 330, 440, "Revenant server", LIFE)
    s.box(330, 90, 290, 78, "Validate, then rate-limit", ["rights box ticked, 16 KiB body cap,", "2 live runs at once"], "life")
    s.box(330, 196, 290, 78, "One host only", ["only the configured archive host;", "off-host redirects are refused"], "life")
    s.box(330, 302, 290, 78, "Rewrite the archive's HTML", ["scripts and handlers removed,", "archive URLs unwrapped"], "life")
    s.box(330, 408, 290, 76, "Serve only the job's own files", ["paths checked against the ZIP,", "content type from an allowlist"], "life")

    s.zone(660, 56, 300, 440, "Content from outside", WRONG)
    s.box(680, 90, 260, 92, "The Internet Archive", ["bytes of pages written by", "strangers, years ago"], "wrong")
    s.box(680, 230, 260, 92, "Preview frame", ["CSP: sandbox; default-src 'none'", "nosniff, no-referrer, noindex"], "life")
    s.box(680, 350, 260, 128, "Browser iframe", ["sandbox=\"\" : no script, no", "forms, no same-origin access.", "It cannot reach /api or other", "jobs."], "life")

    s.arrow(155, 182, 155, 210, DUST)
    s.arrow(270, 136, 330, 130, DUST)
    # The query goes out; the archive's bytes come back through the rewrite, then the server's
    # own file check, then the preview headers, then the sandboxed frame.
    s.arrow(620, 235, 680, 120, WRONG, dashed=True)
    s.arrow(680, 165, 620, 330, WRONG)
    s.arrow(475, 380, 475, 408, LIFE)
    s.arrow(620, 446, 680, 292, LIFE)
    s.arrow(810, 322, 810, 350, LIFE)
    return s.render()


def main():
    out = Path(__file__).resolve().parent
    for name, fn in (("architecture", architecture), ("pipeline", pipeline), ("trust-boundary", trust)):
        (out / f"{name}.svg").write_text(fn(), encoding="utf-8")
    print("wrote", sorted(p.name for p in out.glob("*.svg")))


if __name__ == "__main__":
    main()
