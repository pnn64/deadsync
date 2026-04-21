"""Print per-cell opaque-bounding-box analysis for the submit footer sprites."""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent / "assets" / "graphics" / "submit"


def cell_bbox(im: Image.Image, x0: int, y0: int, w: int, h: int):
    cell = im.crop((x0, y0, x0 + w, y0 + h))
    if cell.mode != "RGBA":
        cell = cell.convert("RGBA")
    alpha = cell.split()[-1]
    bbox = alpha.getbbox()  # (l, t, r, b) of opaque pixels, or None
    return cell, bbox


def report_single(path: Path) -> None:
    im = Image.open(path)
    w, h = im.size
    cell, bbox = cell_bbox(im, 0, 0, w, h)
    if bbox is None:
        print(f"{path.name}: empty image ({w}x{h})")
        return
    l, t, r, b = bbox
    cw = r - l
    ch = b - t
    print(
        f"{path.name}: cell {w}x{h} | content {cw}x{ch} at ({l},{t})-({r},{b}) "
        f"| pad L={l} R={w - r} T={t} B={h - b} "
        f"| fill_w={cw / w:.0%} fill_h={ch / h:.0%}"
    )


def report_sheet(path: Path, cols: int, rows: int) -> None:
    im = Image.open(path)
    sw, sh = im.size
    cw, ch = sw // cols, sh // rows
    # Sample frame 0, the middle frame, and the last frame.
    samples = [0, (cols * rows) // 2, cols * rows - 1]
    print(f"{path.name}: sheet {sw}x{sh}, {cols}x{rows} grid, cell {cw}x{ch}")
    for idx in samples:
        col, row = idx % cols, idx // cols
        x0, y0 = col * cw, row * ch
        _, bbox = cell_bbox(im, x0, y0, cw, ch)
        if bbox is None:
            print(f"  frame {idx:>2}: empty")
            continue
        l, t, r, b = bbox
        bw = r - l
        bh = b - t
        print(
            f"  frame {idx:>2}: content {bw}x{bh} at ({l},{t})-({r},{b}) "
            f"| pad L={l} R={cw - r} T={t} B={ch - b} "
            f"| fill_w={bw / cw:.0%} fill_h={bh / ch:.0%}"
        )


if __name__ == "__main__":
    for name in ("Check_1x1.png", "Refresh_1x1.png", "Rejected_1x1.png"):
        report_single(ROOT / name)
    report_sheet(ROOT / "Hourglass_10x3.png", 10, 3)
    report_sheet(ROOT / "LoadingSpinner_10x3.png", 10, 3)
