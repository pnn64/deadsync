"""Build the DeadSync mouse cursor PNGs from the source reference at
`assets/graphics/cursor/source/cursor.png`.

* `cursor_default.png` — the source silhouette with a 1-pixel white halo
  so the (dark) cursor stays visible on dark backgrounds.
* `cursor_hover.png` — the same silhouette recoloured to the default
  theme's decorative magenta (`#C1006F`) with a darker 1-pixel outline.

Both outputs are 48x48 RGBA. Hotspot is auto-detected at the upper-left
opaque pixel of the downsampled image — the visible tip — and printed at
the end so the value in `src/app/graphics.rs` can be kept in sync.

Run this whenever you tweak the source file or the magenta colour; the
script also drops the freshly-rendered PNGs into
`target/<profile>/assets/graphics/cursor/` so a running game picks them
up without a cargo rebuild.
"""

import os
import shutil
from PIL import Image, ImageFilter

SRC = "assets/graphics/cursor/source/cursor.png"
SIZE = 48
OUT_DIR = "assets/graphics/cursor"

MAGENTA_FILL = (0xC1, 0x00, 0x6F)
MAGENTA_OUTLINE = (0x55, 0x00, 0x30)


def add_halo(img, halo_rgb, thickness=1, alpha=255):
    src_alpha = img.split()[-1]
    dilated = src_alpha.filter(ImageFilter.MaxFilter(2 * thickness + 1))
    if alpha < 255:
        dilated = dilated.point(lambda v: int(v * alpha / 255))
    halo_layer = Image.new("RGBA", img.size, (*halo_rgb, 0))
    halo_layer.putalpha(dilated)
    return Image.alpha_composite(halo_layer, img)


def downsample_reference():
    src = Image.open(SRC).convert("RGBA")
    return src.resize((SIZE, SIZE), Image.LANCZOS)


def recolour(img, rgb):
    r, g, b, a = img.split()
    r = Image.new("L", img.size, rgb[0])
    g = Image.new("L", img.size, rgb[1])
    b = Image.new("L", img.size, rgb[2])
    return Image.merge("RGBA", (r, g, b, a))


def find_hotspot(img):
    """Return the upper-left-most opaque pixel coordinates as the visible tip."""
    alpha = img.split()[-1]
    px = alpha.load()
    for y in range(img.size[1]):
        for x in range(img.size[0]):
            if px[x, y] > 128:
                return (x, y)
    return (0, 0)


def main():
    if not os.path.isfile(SRC):
        raise SystemExit(f"missing source cursor: {SRC}")
    os.makedirs(OUT_DIR, exist_ok=True)
    shape = downsample_reference()

    default = add_halo(shape, (255, 255, 255), thickness=1)
    default.save(f"{OUT_DIR}/cursor_default.png")

    hover = recolour(shape, MAGENTA_FILL)
    hover = add_halo(hover, MAGENTA_OUTLINE, thickness=1)
    hover.save(f"{OUT_DIR}/cursor_hover.png")

    hotspot = find_hotspot(default)
    print(f"wrote {OUT_DIR}/cursor_default.png")
    print(f"wrote {OUT_DIR}/cursor_hover.png")
    print(f"hotspot: ({hotspot[0]}, {hotspot[1]})  (use this in src/app/graphics.rs)")

    for profile in ("debug", "release"):
        t = f"target/{profile}/assets/graphics/cursor"
        if not os.path.isdir(t):
            continue
        for name in ("cursor_default.png", "cursor_hover.png"):
            shutil.copyfile(f"{OUT_DIR}/{name}", f"{t}/{name}")
            print(f"synced -> {t}/{name}")


if __name__ == "__main__":
    main()
