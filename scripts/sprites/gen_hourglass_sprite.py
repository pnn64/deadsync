"""Generate an animated hourglass sprite sheet to match LoadingSpinner_10x3.png.

Output: a 10-column x 3-row PNG of 30 hourglass frames.
- Frames 0..DRAIN_END:   sand drains top -> bottom
- Frames DRAIN_END+1..N: hourglass rotates 180 deg, leaving sand in the
                         "new top" bulb so the loop is seamless.
"""
from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

W = H = 64                     # frame size; tweak to match LoadingSpinner cell
COLS, ROWS, N = 10, 3, 30
DRAIN_END = 22                 # last drain frame; remaining frames are the flip
SUPERSAMPLE = 4                # render at NxSUPERSAMPLE then downscale for AA
GLASS_COLOR = (255, 255, 255, 255)
SAND_COLOR  = (255, 255, 255, 255)
OUTLINE_THICK = 4
CURVE_SAMPLES = 96             # silhouette resolution per side
CURVE_POWER = 0.55             # < 1 rounds the bulbs and sharpens the neck

CX = W // 2
TOP_Y = 5
BOT_Y = H - 5
MID_Y = H // 2
HALF_W = 27                    # bulb half-width
NECK_W = 2                     # narrow waist half-width


def _bulb_shape(y_norm: float) -> float:
    """Cosine raised to a fractional power: rounder bulbs, sharper neck.

    `y_norm` is in [0, 1] with 0 at the top, 0.5 at the neck, 1 at the bottom.
    Returns a value in [0, 1] where 0 is fully pinched (neck) and 1 is the
    full bulb half-width.
    """
    s = (1.0 + math.cos(2.0 * math.pi * y_norm)) * 0.5  # 1 at top/bot, 0 at neck
    return s ** CURVE_POWER


def profile_half_width(y: float) -> float:
    y_norm = (y - TOP_Y) / (BOT_Y - TOP_Y)
    return NECK_W + (HALF_W - NECK_W) * _bulb_shape(y_norm)


def right_curve(y_start: float, y_end: float, samples: int = CURVE_SAMPLES) -> list[tuple[float, float]]:
    pts: list[tuple[float, float]] = []
    for i in range(samples + 1):
        t = i / samples
        y = y_start + t * (y_end - y_start)
        pts.append((CX + profile_half_width(y), y))
    return pts


def mirror(pts: list[tuple[float, float]]) -> list[tuple[float, float]]:
    return [(2 * CX - x, y) for x, y in reversed(pts)]


def stroke_polyline(d: ImageDraw.ImageDraw, pts: list[tuple[float, float]]) -> None:
    for a, b in zip(pts, pts[1:]):
        d.line([a, b], fill=GLASS_COLOR, width=OUTLINE_THICK)


def _erode_mask(mask: Image.Image, pixels: int) -> Image.Image:
    """Iteratively erode `mask` by `pixels` (PIL's MinFilter caps at size 9 → 4 px)."""
    remaining = pixels
    while remaining > 0:
        step = min(4, remaining)
        mask = mask.filter(ImageFilter.MinFilter(step * 2 + 1))
        remaining -= step
    return mask


def draw_hourglass(sand_top: float, sand_bot: float, rotation_deg: float) -> Image.Image:
    s = SUPERSAMPLE
    big_w, big_h = W * s, H * s
    big_cx = CX * s
    big_top, big_bot, big_mid = TOP_Y * s, BOT_Y * s, MID_Y * s
    big_half_w, big_neck = HALF_W * s, NECK_W * s
    big_thick = OUTLINE_THICK * s

    def hw(y: float) -> float:
        y_norm = (y - big_top) / (big_bot - big_top)
        return big_neck + (big_half_w - big_neck) * _bulb_shape(y_norm)

    def rcurve(y0: float, y1: float, n: int = CURVE_SAMPLES) -> list[tuple[float, float]]:
        return [
            (big_cx + hw(y0 + (i / n) * (y1 - y0)), y0 + (i / n) * (y1 - y0))
            for i in range(n + 1)
        ]

    def mir(pts: list[tuple[float, float]]) -> list[tuple[float, float]]:
        return [(2 * big_cx - x, y) for x, y in reversed(pts)]

    # ---- Outline as a single closed silhouette -> erosion ring ----
    # Build the full outer boundary as one closed polygon. PIL renders polygon
    # fills with antialiasing once we downscale, and computing the ring as
    # (outer_mask - eroded_inner_mask) avoids the jagged corners that came from
    # drawing the cap line and silhouette as separate strokes.
    silhouette_right = rcurve(big_top, big_bot)
    silhouette_pts = silhouette_right + mir(silhouette_right)

    outer_mask = Image.new("L", (big_w, big_h), 0)
    ImageDraw.Draw(outer_mask).polygon(silhouette_pts, fill=255)
    inner_mask = _erode_mask(outer_mask, big_thick)
    ring_alpha = ImageChops.subtract(outer_mask, inner_mask)
    ring_buf = Image.new("RGBA", (big_w, big_h), GLASS_COLOR)
    ring_buf.putalpha(ring_alpha)

    # ---- Sand fills, masked to stay inside the inner cavity ----
    sand_buf = Image.new("RGBA", (big_w, big_h), (0, 0, 0, 0))
    sand_d = ImageDraw.Draw(sand_buf)

    surface_y_b: float | None = None

    if sand_top > 0.01:
        surface_y = big_top + (1 - sand_top) * (big_mid - big_top)
        right = rcurve(surface_y, big_mid)
        sand_d.polygon(right + mir(right), fill=SAND_COLOR)

    if sand_bot > 0.01:
        surface_y_b = big_bot - sand_bot * (big_bot - big_mid)
        right = rcurve(surface_y_b, big_bot)
        sand_d.polygon(right + mir(right), fill=SAND_COLOR)

    if 0.02 < sand_top < 0.99:
        stream_bot = surface_y_b - 1 if surface_y_b is not None else big_bot - 4 * s
        sand_d.line(
            [(big_cx, big_mid), (big_cx, int(stream_bot))],
            fill=SAND_COLOR,
            width=2 * s,
        )

    sr, sg, sb, sa = sand_buf.split()
    sa = ImageChops.multiply(sa, inner_mask)
    sand_buf = Image.merge("RGBA", (sr, sg, sb, sa))

    composed = Image.alpha_composite(sand_buf, ring_buf)

    if rotation_deg != 0:
        composed = composed.rotate(
            -rotation_deg, resample=Image.BICUBIC, center=(big_cx, big_h // 2)
        )

    return composed.resize((W, H), resample=Image.LANCZOS)


def main() -> None:
    sheet = Image.new("RGBA", (W * COLS, H * ROWS), (0, 0, 0, 0))
    for i in range(N):
        if i <= DRAIN_END:
            t = i / DRAIN_END
            # Ease so the start/end of drain feel less linear.
            t_eased = t * t * (3 - 2 * t)
            sand_top = 1.0 - t_eased
            sand_bot = t_eased
            rot = 0.0
        else:
            t = (i - DRAIN_END) / (N - DRAIN_END)
            sand_top = 0.0
            sand_bot = 1.0
            rot = 180.0 * t

        frame = draw_hourglass(sand_top, sand_bot, rot)
        col, row = i % COLS, i // COLS
        sheet.paste(frame, (col * W, row * H), frame)

    out = Path(__file__).resolve().parent.parent.parent / "assets" / "graphics" / "submit" / "Hourglass_10x3.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out)
    print(f"wrote {out} ({sheet.size[0]}x{sheet.size[1]})")


if __name__ == "__main__":
    main()
