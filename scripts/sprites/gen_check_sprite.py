"""Generate a single-frame checkmark sprite for the submit footer.

Output: a 1-column x 1-row 64x64 PNG with a white, anti-aliased check stroke
on a transparent background. Rendered as a sprite (not text) so the renderer
can tint it with `diffuse(...)` to indicate success state without depending
on font glyph coverage.
"""
from __future__ import annotations

import math
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
    half = big_thick / 2.0

    def to_big(p: tuple[int, int]) -> tuple[float, float]:
        return (p[0] * SUPERSAMPLE, p[1] * SUPERSAMPLE)

    bp0, bp1, bp2 = to_big(p0), to_big(p1), to_big(p2)

    def stroke_quad(a, b):
        # Rectangle of width `big_thick` centered on segment a->b with flat
        # (butt) end caps perpendicular to the segment direction.
        ax, ay = a
        bx, by = b
        dx, dy = bx - ax, by - ay
        length = math.hypot(dx, dy)
        nx, ny = -dy / length, dx / length  # unit normal
        ox, oy = nx * half, ny * half
        return [
            (ax + ox, ay + oy),
            (bx + ox, by + oy),
            (bx - ox, by - oy),
            (ax - ox, ay - oy),
        ]

    # Two flat-ended rectangles for the two arms.
    draw.polygon(stroke_quad(bp0, bp1), fill=COLOR)
    draw.polygon(stroke_quad(bp1, bp2), fill=COLOR)

    # Fill the outer corner of the elbow so the joint reads as a clean angle
    # rather than two overlapping rectangles with a notch on the outside.
    def perp_unit(a, b):
        ax, ay = a
        bx, by = b
        dx, dy = bx - ax, by - ay
        length = math.hypot(dx, dy)
        return (-dy / length, dx / length)

    n01 = perp_unit(bp0, bp1)
    n12 = perp_unit(bp1, bp2)
    # Outer side is the side away from the check's interior. For the V-shape
    # at p1 the outer side is "below" (positive y); both normals' signs need
    # to point that way. Just emit both candidates and let the polygon fill
    # cover the elbow gap from each side.
    for sign in (1.0, -1.0):
        outer1 = (bp1[0] + n01[0] * half * sign, bp1[1] + n01[1] * half * sign)
        outer2 = (bp1[0] + n12[0] * half * sign, bp1[1] + n12[1] * half * sign)
        draw.polygon([bp1, outer1, outer2], fill=COLOR)

    return img.resize((W, H), resample=Image.LANCZOS)


def main() -> None:
    sheet = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    sheet.paste(draw_check(), (0, 0))
    out = (
        Path(__file__).resolve().parent.parent.parent
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
