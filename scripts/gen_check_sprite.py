"""Generate a single-frame checkmark sprite for the submit footer.

Output: a 1-column x 1-row 64x64 PNG with a white, anti-aliased check stroke
on a transparent background. Rendered as a sprite (not text) so the renderer
can tint it with `diffuse(...)` to indicate success state without depending
on font glyph coverage.
"""
from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw

W = H = 64
SUPERSAMPLE = 4
STROKE_THICK = 11   # logical pixels; multiplied by SUPERSAMPLE for the big draw
COLOR = (255, 255, 255, 255)


def draw_check() -> Image.Image:
    big_w = W * SUPERSAMPLE
    big_h = H * SUPERSAMPLE
    img = Image.new("RGBA", (big_w, big_h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Three control points laid out in a 64x64 logical frame:
    #   p0: upper-left where the short arm starts
    #   p1: bottom-center elbow
    #   p2: upper-right where the long arm ends
    # The long arm is roughly twice the short arm so the shape reads as a
    # checkmark rather than a wide V.
    p0 = (11, 32)
    p1 = (27, 50)
    p2 = (53, 14)

    big_thick = STROKE_THICK * SUPERSAMPLE

    def to_big(p: tuple[int, int]) -> tuple[int, int]:
        return (p[0] * SUPERSAMPLE, p[1] * SUPERSAMPLE)

    bp0, bp1, bp2 = to_big(p0), to_big(p1), to_big(p2)

    # Draw the two segments with the same stroke width and round end-caps via
    # circles at every joint so the elbow stays clean.
    draw.line([bp0, bp1], fill=COLOR, width=big_thick)
    draw.line([bp1, bp2], fill=COLOR, width=big_thick)
    r = big_thick // 2
    for cx, cy in (bp0, bp1, bp2):
        draw.ellipse((cx - r, cy - r, cx + r, cy + r), fill=COLOR)

    return img.resize((W, H), resample=Image.LANCZOS)


def main() -> None:
    sheet = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    sheet.paste(draw_check(), (0, 0))
    out = (
        Path(__file__).resolve().parent.parent
        / "assets"
        / "graphics"
        / "submit"
        / "Check_1x1.png"
    )
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out)
    print(f"wrote {out} ({sheet.size[0]}x{sheet.size[1]})")


if __name__ == "__main__":
    main()
