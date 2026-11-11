"""Render the Revenant mark (public/favicon.svg) to PNG and ICO sizes.

Usage: python3 scripts/make_icons.py   (from apps/web). Needs Pillow.
The drawing here must match public/favicon.svg.
"""

from pathlib import Path

from PIL import Image, ImageDraw

BONE = (232, 227, 210, 255)
INK = (11, 12, 10, 255)
S = 24  # supersampling scale on a 32-unit grid


def mark(background=None, inset=0.0, stroke=2.2):
    """Return the mark at 32*S pixels. `inset` shrinks it toward the center (0..0.4)."""
    size = 32 * S
    img = Image.new("RGBA", (size, size), background or (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    scale = 1.0 - inset * 2
    off = size * inset

    def p(x, y):
        return (off + x * S * scale, off + y * S * scale)

    # Tombstone: a rectangle with a semicircular top.
    d.pieslice([*p(6, 4), *p(26, 24)], 180, 360, fill=BONE)
    d.rectangle([*p(6, 14), *p(26, 30)], fill=BONE)

    # The line: flat, one pulse, flat.
    # Keep the round end caps inside the slab (its edges are x=6 and x=26).
    left = 6 + stroke / 2 + 0.05
    right = 26 - stroke / 2 - 0.05
    pts = [p(left, 20), p(13, 20), p(15.2, 13.5), p(18.4, 25), p(20.7, 20), p(right, 20)]
    width = int(stroke * S * scale)
    d.line(pts, fill=INK, width=width, joint="curve")
    r = width / 2
    for x, y in (pts[0], pts[-1]):
        d.ellipse([x - r, y - r, x + r, y + r], fill=INK)
    return img


def main():
    out = Path(__file__).resolve().parent.parent / "public"
    out.mkdir(exist_ok=True)

    base = mark()
    base.resize((32, 32), Image.LANCZOS).save(out / "favicon-32.png")
    # At 16 px the line needs more weight to survive downsampling.
    mark(stroke=3.0).resize((16, 16), Image.LANCZOS).save(out / "favicon-16.png")

    # iOS ignores transparency, so the touch icon sits on the page color.
    touch = mark(background=INK, inset=0.14)
    touch.resize((180, 180), Image.LANCZOS).save(out / "apple-touch-icon.png")

    base.resize((48, 48), Image.LANCZOS).save(
        out / "favicon.ico", format="ICO", sizes=[(16, 16), (32, 32), (48, 48)]
    )
    # Note: the ICO's 16 px frame is resampled from the 48 px image by Pillow.
    print("wrote", sorted(p.name for p in out.iterdir()))


if __name__ == "__main__":
    main()
