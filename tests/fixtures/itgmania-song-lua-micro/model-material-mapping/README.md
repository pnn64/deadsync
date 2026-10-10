This independent ITGmania actor scene retains six native Model draws:
diffuse, additive and glow for a diffuse-sphere model, then the same passes
for a secondary-sphere model. It uses ITGmania `5c737928` and the pinned
harness 0.1.42 executable from `a80f15b`. `native.json` includes every draw,
input SHA-256 hashes, source and executable provenance, and the raw hash.

Reproduce with `itgmania-harness-rs actor-conformance control.json --out
native-raw.json`. The saved successful capture was postprocessed after
correcting a lowercase actor-kind filter; its elapsed time was not retained.

The folder name deliberately avoids `sphere`: native `AnimatedTexture::Load`
in `ModelTypes.cpp` searches the entire material path for that substring.
Only the intended diffuse or secondary INI filename contains it. The native
states are `[true, false, false]` and `[false, true, false]`, respectively.

`native_model_material_mapping_matches_production` runs the paired Lua
through the production compiler and both Model builders, checking all 36
submitted vertex sphere flags against native pass state. The two materials
share positions and UVs, so their geometry keys must also distinguish
normal and sphere flags. Diffuse/additive geometry is precomputed before
gameplay; the existing prewarm regressions require retained Arc reuse.

The native command capture observes sphere state before GPU texgen. This
test does not compare generated sphere coordinates or framebuffer pixels.
Sampler filtering, per-image atlas wrapping, lighting and full-song Model
acceptance remain open.
