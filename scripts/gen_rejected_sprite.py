"""Generate a single-frame "rejected" sprite for the submit footer.

Output: a 1-column x 1-row 64x64 PNG with a white, anti-aliased circle
crossed by a single diagonal slash on a transparent background. Stand-in
for the unicode ⊘ glyph, which the miso font does not include.
"""
from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw

W = H = 64
SUPERSAMPLE = 4
STROKE_THICK = 6
COLOR = (255, 255, 255, 255)


def draw_rejected() -> Image.Image:
    big_w = W * SUPERSAMPLE
    big_h = H * SUPERSAMPLE
    img = Image.new("RGBA", (big_w, big_h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    cx, cy = big_w // 2, big_h // 2
    radius = int(26 * SUPERSAMPLE)
    big_thick = STROKE_THICK * SUPERSAMPLE

    # Outer circle.
    bbox = (cx - radius, cy - radius, cx + radius, cy + radius)
    draw.ellipse(bbox, outline=COLOR, width=big_thick)

    # Diagonal slash from upper-left to lower-right of the inscribed area.
    inset = big_thick // 2
    inner_r = radius - inset
    angle = math.radians(45.0)
    dx = inner_r * math.cos(angle)
    dy = inner_r * math.sin(angle)
    p_a = (cx - dx, cy - dy)
    p_b = (cx + dx, cy + dy)
    draw.line([p_a, p_b], fill=COLOR, width=big_thick)
    r = big_thick // 2
    for px, py in (p_a, p_b):
        draw.ellipse((px - r, py - r, px + r, py + r), fill=COLOR)

    return img.resize((W, H), resample=Image.LANCZOS)


def main() -> None:
    sheet = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    sheet.paste(draw_rejected(), (0, 0))
    out = (
        Path(__file__).resolve().parent.parent
        / "assets"
        / "graphics"
        / "submit"
        / "Rejected_1x1.png"
    )
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out)
    print(f"wrote {out} ({sheet.size[0]}x{sheet.size[1]})")


if __name__ == "__main__":
    main()
