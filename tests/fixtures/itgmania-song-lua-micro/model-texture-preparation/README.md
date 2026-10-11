These 21 controls compare production PNG decoding and Model image preparation
with the compiled, pinned ITGmania ApplyHotPinkColorKey, Zoom and FixHiddenAlpha
routines. `native.json` is the harness 0.1.45 output; each `.rgba` is its exact
row-major output. `cases.json` specifies the production decode parameters.

The controls cover opaque and partial-alpha pink, top/bottom versus side-edge
off-pink selection, duplicate indexed entries, packed palettes, palette alpha,
16-bit high-byte stripping, low-bit grayscale expansion, hidden RGB, iterative
resizing, odd dimensions, the Model 2048 size limit and one-pixel expansion.
PNG encodings preserve the explicit decoded inputs in `control.json`.

Native ModelTypes requests a separate RageTextureID with stretch and color key
enabled. RageBitmapTexture keys before resizing and fixes hidden RGB afterward.
DeadSync keeps this prepared image separate from the Sprite view of its source.

This is CPU image evidence. The native PNG file loader is source-audited but
not executed here. Full bitmap format conversion, settings/device size policy,
indexed GIF/BMP loading, physical mip storage and native framebuffer pixels
remain pending. These controls do not establish complete song Lua parity.
