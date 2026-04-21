"""Generate an animated hourglass sprite sheet to match LoadingSpinner_10x3.png.

Output: a 10-column x 3-row PNG of 30 hourglass frames.
- Frames 0..DRAIN_END:   sand drains top -> bottom
- Frames DRAIN_END+1..N: hourglass rotates 180 deg, leaving sand in the
                         "new top" bulb so the loop is seamless.
"""
from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw

W = H = 64                     # frame size; tweak to match LoadingSpinner cell
COLS, ROWS, N = 10, 3, 30
DRAIN_END = 22                 # last drain frame; remaining frames are the flip
SUPERSAMPLE = 4                # render at NxSUPERSAMPLE then downscale for AA
GLASS_COLOR = (255, 255, 255, 255)
SAND_COLOR  = (255, 255, 255, 255)
OUTLINE_THICK = 2
CURVE_SAMPLES = 96             # silhouette resolution per side

CX = W // 2
TOP_Y = 8
BOT_Y = H - 8
MID_Y = H // 2
HALF_W = 22                    # bulb half-width
NECK_W = 3                     # narrow waist half-width

# Cosine silhouette: x(y) = a + b * cos(2 * pi * y_norm)
# y_norm = 0 -> top (wide); 0.5 -> waist; 1 -> bottom (wide).
_PROFILE_A = (HALF_W + NECK_W) / 2
_PROFILE_B = (HALF_W - NECK_W) / 2


def profile_half_width(y: float) -> float:
    y_norm = (y - TOP_Y) / (BOT_Y - TOP_Y)
    return _PROFILE_A + _PROFILE_B * math.cos(2 * math.pi * y_norm)


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


def draw_hourglass(sand_top: float, sand_bot: float, rotation_deg: float) -> Image.Image:
    s = SUPERSAMPLE
    big_w, big_h = W * s, H * s
    big_cx = CX * s
    big_top, big_bot, big_mid = TOP_Y * s, BOT_Y * s, MID_Y * s
    big_half_w, big_neck = HALF_W * s, NECK_W * s
    big_thick = OUTLINE_THICK * s
    big_a = (big_half_w + big_neck) / 2
    big_b = (big_half_w - big_neck) / 2

    def hw(y: float) -> float:
        y_norm = (y - big_top) / (big_bot - big_top)
        return big_a + big_b * math.cos(2 * math.pi * y_norm)

    def rcurve(y0: float, y1: float, n: int = CURVE_SAMPLES) -> list[tuple[float, float]]:
        return [
            (big_cx + hw(y0 + (i / n) * (y1 - y0)), y0 + (i / n) * (y1 - y0))
            for i in range(n + 1)
        ]

    def mir(pts: list[tuple[float, float]]) -> list[tuple[float, float]]:
        return [(2 * big_cx - x, y) for x, y in reversed(pts)]

    def stroke(d: ImageDraw.ImageDraw, pts: list[tuple[float, float]]) -> None:
        for a, b in zip(pts, pts[1:]):
            d.line([a, b], fill=GLASS_COLOR, width=big_thick)

    buf = Image.new("RGBA", (big_w, big_h), (0, 0, 0, 0))
    d = ImageDraw.Draw(buf)

    surface_y_b: float | None = None

    if sand_top > 0.01:
        surface_y = big_top + (1 - sand_top) * (big_mid - big_top)
        right = rcurve(surface_y, big_mid)
        d.polygon(right + mir(right), fill=SAND_COLOR)

    if sand_bot > 0.01:
        surface_y_b = big_bot - sand_bot * (big_bot - big_mid)
        right = rcurve(surface_y_b, big_bot)
        d.polygon(right + mir(right), fill=SAND_COLOR)

    if 0.02 < sand_top < 0.99:
        stream_bot = surface_y_b - 1 if surface_y_b is not None else big_bot - 4 * s
        d.line([(big_cx, big_mid), (big_cx, stream_bot)], fill=SAND_COLOR, width=2 * s)

    silhouette = rcurve(big_top, big_bot)
    stroke(d, silhouette)
    stroke(d, mir(silhouette))
    d.line([(big_cx - big_half_w, big_top), (big_cx + big_half_w, big_top)], fill=GLASS_COLOR, width=big_thick)
    d.line([(big_cx - big_half_w, big_bot), (big_cx + big_half_w, big_bot)], fill=GLASS_COLOR, width=big_thick)

    if rotation_deg != 0:
        buf = buf.rotate(-rotation_deg, resample=Image.BICUBIC, center=(big_cx, big_h // 2))

    return buf.resize((W, H), resample=Image.LANCZOS)


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

    out = Path(__file__).resolve().parent.parent / "assets" / "graphics" / "submit" / "Hourglass_10x3.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out)
    print(f"wrote {out} ({sheet.size[0]}x{sheet.size[1]})")


if __name__ == "__main__":
    main()
