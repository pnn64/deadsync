"""Generate a single-frame "refresh / press-F5" sprite for the submit footer.

Output: a 1-column x 1-row 64x64 PNG with a white, anti-aliased circular
arrow (clockwise) on a transparent background. Stand-in for the unicode
↻ glyph, which the miso font does not include.

The shape is rendered as a filled mask (outer disk − inner disk − wedge cutout)
with a triangular arrowhead added on top, then downscaled with LANCZOS. This
keeps the stroke ends, the elbow, and the arrowhead all anti-aliased as one
silhouette instead of separate strokes that would alias against each other.
"""
from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw

W = H = 64
SUPERSAMPLE = 4
RADIUS_LOGICAL = 21       # mid-line radius of the ring
STROKE_LOGICAL = 6        # ring thickness
GAP_DEG = 60.0            # wedge of arc removed for the arrow's head/tail gap
HEAD_LEN_LOGICAL = 14     # length of the arrowhead from base to tip
HEAD_WIDTH_LOGICAL = 16   # full width of the arrowhead base
COLOR = (255, 255, 255, 255)


def draw_refresh() -> Image.Image:
    big_w = W * SUPERSAMPLE
    big_h = H * SUPERSAMPLE
    cx, cy = big_w // 2, big_h // 2

    radius = RADIUS_LOGICAL * SUPERSAMPLE
    half_stroke = (STROKE_LOGICAL * SUPERSAMPLE) / 2.0
    head_len = HEAD_LEN_LOGICAL * SUPERSAMPLE
    head_half = (HEAD_WIDTH_LOGICAL * SUPERSAMPLE) / 2.0

    # Build the ring as a mask: outer disk minus inner disk minus wedge cutout.
    # The wedge is centered on +X (3 o'clock) so the gap sits on the right side
    # where the arrowhead lives.
    ring = Image.new("L", (big_w, big_h), 0)
    rd = ImageDraw.Draw(ring)
    outer_r = radius + half_stroke
    inner_r = radius - half_stroke
    rd.ellipse(
        (cx - outer_r, cy - outer_r, cx + outer_r, cy + outer_r), fill=255,
    )
    rd.ellipse(
        (cx - inner_r, cy - inner_r, cx + inner_r, cy + inner_r), fill=0,
    )

    # Cut out a wedge for the gap. Pillow measures angles clockwise from +X.
    wedge_half = GAP_DEG / 2.0
    cutout_r = outer_r + 4 * SUPERSAMPLE
    rd.pieslice(
        (cx - cutout_r, cy - cutout_r, cx + cutout_r, cy + cutout_r),
        start=-wedge_half, end=wedge_half, fill=0,
    )

    # Arrowhead at the top edge of the gap (start of the clockwise sweep), with
    # its tip pointing clockwise so it visually leads the arc.
    tip_angle_deg = -wedge_half          # top edge of the cutout
    tip_angle = math.radians(tip_angle_deg)
    base_cx = cx + radius * math.cos(tip_angle)
    base_cy = cy + radius * math.sin(tip_angle)
    # Tangent that continues the arc clockwise from the top edge points
    # down-right; this is the direction the arrowhead should face so it leads
    # the spin.
    tan_dx = -math.sin(tip_angle)
    tan_dy = math.cos(tip_angle)
    # Radial (outward) for the base width:
    nrm_dx = math.cos(tip_angle)
    nrm_dy = math.sin(tip_angle)

    p_tip = (base_cx + tan_dx * head_len, base_cy + tan_dy * head_len)
    p_base_out = (base_cx + nrm_dx * head_half, base_cy + nrm_dy * head_half)
    p_base_in = (base_cx - nrm_dx * head_half, base_cy - nrm_dy * head_half)
    rd.polygon([p_tip, p_base_out, p_base_in], fill=255)

    # Render the white shape using the mask alpha.
    img = Image.new("RGBA", (big_w, big_h), (255, 255, 255, 0))
    img.putalpha(ring)
    return img.resize((W, H), resample=Image.LANCZOS)


def main() -> None:
    sheet = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    sheet.paste(draw_refresh(), (0, 0))
    out = (
        Path(__file__).resolve().parent.parent
        / "assets"
        / "graphics"
        / "submit"
        / "Refresh_1x1.png"
    )
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out)
    print(f"wrote {out} ({sheet.size[0]}x{sheet.size[1]})")


if __name__ == "__main__":
    main()
