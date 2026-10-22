The current `native.json` was regenerated with harness **0.1.95**. See
[`native-capture-manifest.json`](../native-capture-manifest.json) for its
request, resources, binary and source revision. Version-specific capture notes
below describe the earlier capture; numerical observations were retained.

Native CPU surface preprocessing from a clean harness 0.1.44 capture.

`control.json` supplies decoded RGB, RGBA and paletted surfaces. The linked
pinned ITGmania routines perform ApplyHotPinkColorKey, Zoom and
FixHiddenAlpha. `native.json` records all 12 outputs. Source PNGs retain the
exact RGBA inputs for the three hidden-alpha cases; `.rgba` files contain
their native output bytes for production-decoder comparisons.

The transparent case has no visible pixels. Native FindAlphaRGB returns
black in both directions and SetAlphaRGB clears all hidden RGB values.
The other controls cover a uniform visible edge and mixed visible edges.

Native color-key and resize bytes are CPU subroutine evidence. Effective
bitmap texture policy, GPU mip levels and framebuffer output remain open.
The Model request/dimension control is separate. These micro fixtures do
not establish complete full-song Model parity.
