# JPEG application metadata

Raw APPn/COM payloads remain borrowed and retain their marker order and scan
position. Optional typed views validate JFIF density/thumbnail fields, JFXX
JPEG/indexed/RGB thumbnail framing, Adobe version/flags/transform, Exif TIFF
directories and values, and ICC chunk framing. Unknown payloads remain intact.
Malformed optional metadata produces a typed-query error and remains available
for raw rewriting; it does not prevent JPEG coefficient/pixel decoding.

## Profiles and encoding

`CoefficientImage::icc_profile` returns an ordered borrowed view, including
profiles whose APP2 chunks appear out of sequence. It rejects missing,
duplicate, inconsistent, zero-indexed and post-SOS chunks. One-chunk profiles
can be borrowed contiguously; `chunks`, `copy_to` and explicitly bounded
`to_vec` support larger profiles. `header` reads the ICC header across chunk
boundaries without allocating. It checks the signature, declared byte size,
and rendering intent, and exposes class/color-space/PCS/version fields.
ICC tag processing and color-management transforms use the original profile
exposed to the caller; the JPEG sample grid retains its declared color space.

`MetadataOptions` adds borrowed prefix APPn/COM segments to coefficient or pixel
encoding. `IccWrite::{Preserve,Remove,Replace}` controls embedded profiles.
Replacement removes prior ICC chunks and writes canonical sequence/count
headers before SOS, with no full-segment intermediate buffer. The maximum
chunk payload is 65,519 bytes and the maximum embedded profile is 16,707,345
bytes (255 chunks), subject to the configured metadata/output budgets.
Raw payload rewriting preserves malformed optional application bytes when
`Preserve` is selected. Low-level additions are checked for marker framing and
resource limits; typed metadata validation is available separately.

```rust
use deadlib_image_jpeg::{CodingOptions, IccWrite, MetadataOptions,
    encode_coefficients_with_metadata};
# fn example(image: &deadlib_image_jpeg::CoefficientImage<'_>, profile: &[u8])
# -> deadlib_image_jpeg::Result<Vec<u8>> {
let metadata = MetadataOptions {
    icc: IccWrite::Replace(profile),
    ..MetadataOptions::default()
};
encode_coefficients_with_metadata(image, CodingOptions::default(), metadata)
# }
```

Exif views support both TIFF byte orders, inline values, bounded external
values, generic IFD access, orientation and IFD1 JPEG thumbnail references.
Unknown tags retain their raw type/count. Unsupported field encodings return
an explicit error when their values are requested. Thumbnail traversal visits
at most two IFDs and does not follow arbitrary recursive chains. Pixel
orientation is exposed to the caller. JFXX embedded JPEG bytes can be passed
to the JPEG decoder separately when a thumbnail is needed.

JFIF and Adobe color hints are taken before the first SOS. JFIF three-channel
images use YCbCr, including the compatibility case of a contradictory Adobe
hint. Late color markers remain available as raw/typed segments and do not
relabel already coded components. The marker policy matches libjpeg-turbo for
the tested cases.

## External evidence

```text
cmake --build <oracle-build> --config Release
cargo build -p deadlib-image-jpeg --example metadata
python crates/deadlib-image/jpeg/tests/reference_metadata.py --oracle <jpeg-metadata> --native target/debug/examples/metadata.exe --cjpeg <cjpeg> --out crates/deadlib-image/jpeg/experiments/metadata-gate
```

The C client uses libjpeg-turbo 3.2.1's marker and ICC APIs. **114 valid cases**
pass exact prefix-marker/payload/order, ICC data, JFIF density, Adobe transform
and color-space comparisons. Cases include both precisions, sequential and
progressive rewriting, profile preserve/remove/replace, chunk boundaries,
255-chunk maximum profiles, reordered chunks, all JFXX thumbnail forms, both
Exif byte orders, and early/late/conflicting color hints. Pillow 12.3.0 also
reads Exif orientation correctly after native rewriting. The saved sRGB profile
is accepted by Pillow/LittleCMS 2.19 after native extraction.

Six malformed ICC sequences are rejected by both metadata readers. The C API
returns no profile and can issue `bad ICC marker`; the Rust typed API returns
an explicit error. Raw JPEG decoding and preservation still work. Retained
case hashes and profile hashes are in tests/reports/metadata-turbo.json.

Rust tests cover all typed headers, truncated thumbnails/Exif pointers, ICC
missing/duplicate/count/order errors, exact boundary sizes through 255 chunks,
allocation budgets, a real sRGB header, late color tags, and 5,000 deterministic
malformed Exif mutations. Coverage-guided fuzzing remains in the overall plan.

Normative references:
- [JFIF 1.02](https://www.w3.org/Graphics/JPEG/jfif3.pdf)
- [ICC profile embedding, Annex B](https://www.color.org/icc1v42.pdf)
- [ICC embedding overview](https://www.color.org/profile_embedding/)
- [CIPA Exif standards](https://www.cipa.jp/e/std/std-sec.html)

This metadata milestone does not close the remaining codec process, streaming,
independent corpus, fuzzing, or performance requirements listed in COVERAGE.md.
