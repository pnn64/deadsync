# Native Sprite preparation controls

Harness `0.1.50` executes the pinned ITGmania bitmap loader on ten inputs
with varied color and alpha. `native.json` retains decoded source pixels,
native source/image/allocation dimensions, frame rectangles, and nine
complete prepared upload images: 68,608 RGBA bytes. Two independent native
captures are byte-identical. `provenance.json` records their identities.

The controls cover 3x2, 9x3, 3x9 and 1x1 sources, explicit stretch, low
resolution, a 2049x3 capped source, a resolution hint and a frame grid.
Native minimum-size preparation stretches both image axes when either
power-of-two allocation would be below eight pixels. Source dimensions
remain logical; the 3x2 source has an 8x8 prepared image, and the resolution
hint gives a 53x12 logical source with the same 8x8 prepared image.

The 7x9 control has an 8x16 allocation with unused padding. Its native
metadata is retained and its upload pixels are excluded. For full images,
the capture reloads an existing native texture and checks its freshly
computed image dimensions inside the upload callback before reading any
pixels. It does not reproduce the sizing policy to decide capture safety.

Run the portable control with the pinned harness:

    itgmania-harness-rs actor-conformance control.json --out reproduced.json

These are native CPU upload surfaces. Physical tiny Sprite uploads in
DeadSync remain open: its ordinary decoder still returns raw tiny sizes.
The repair must preserve logical source geometry and check image-coordinate
offsets and normalized UVs together with prepared pixels. Native framebuffer,
GPU mip storage, and the rest of the 501-source/492-context corpus remain open.
