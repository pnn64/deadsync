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

DeadSync `20e14e048` applies native sizing to ordinary Sprite uploads while
retaining logical source dimensions. Its default high-resolution profile
checks nine controls: eight complete images (68,352 native RGBA bytes)
and the padded NPOT metadata control. `prepared-pixels.json` identifies
the exported `.rgba` byte arrays. Production software upload readback,
startup jobs, replacement handling, logical bindings and tiny image
offsets are verified; 1,437 domain tests pass.

These are native CPU upload surfaces. The high-resolution-disabled control
is retained but its alternate production profile is unproven. Complete
NPOT coordinate/sampling behavior, native GPU framebuffer output, mip
storage and the rest of the 501-source/492-context corpus remain open.
