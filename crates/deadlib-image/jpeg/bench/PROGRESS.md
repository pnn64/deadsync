# JPEG optimization checkpoint

The performance goal remains active. The native codec is still slower than
`zune-jpeg` and `image` in the initial comparison; this is a development checkpoint.

Retained changes:

- Exact Q20 IDCT factorization using even/odd and mirror symmetry. The original
  rounding boundaries, clipping, differential behavior, and wide intermediates
  remain intact. A direct matrix comparison covers sparse and wide inputs.
- Exact paired 8-bit 4:2:0 interpolation and color conversion. Sliding chroma
  values remove repeated coordinate divisions without an intermediate rounding
  or an extra allocation. Odd dimensions, clamped edges, and scaled sampling
  factors are checked against the generic sampler.
- An independent mixed-width bit-reader regression covers stuffing and long
  streams. The attempted four-byte refill was removed after an A/B comparison
  showed about 5% more CPU cycles overall.

The initial corpus consists of all five repository JPEG assets and the native
sequential/progressive fixture directories. It currently measures 31 8-bit
inputs. The 24 higher-precision cases remain unmeasured while the precision
adapters are developed. All three reference output adapters now cover the
31 inputs, including CMYK-to-RGB conversion for `jpeg-decoder`.

Preliminary per-file geometric mean native/reference process-cycle ratios:

| Reference | Original native source | Current optimized source |
| --- | --- | --- |
| zune-jpeg | 2.4588 | 1.5489 |
| jpeg-decoder | 1.4338 | 0.9160 |
| image | 2.0752 | 1.3221 |

The original-source run used 10 ms batches; the optimized comparison used 20 ms.
These are exploratory results, including worker costs, rather than final
performance acceptance measurements. Every measured native output matched the
original native pixel snapshot exactly. The existing full suite, including
official ITU transforms and independent coefficient/sample fixtures, passes
167 tests after the retained changes. Strict Clippy also passes.

Allocation measurements identify the next structural problem: for the
1920x1080 `janus5kshopd.jpg` asset, the current three-stage pipeline makes 12
calls requesting 18,709,572 bytes; zune makes 21 calls requesting 6,544,908 bytes.
Full coefficients and native-precision planes outlive reconstruction until RGB
output exists. A matched pixel-output API should avoid those full-image
intermediates while retaining the existing coefficient and planar APIs.

Remaining work includes reducing pixel-decoding storage, further entropy/IDCT
and conversion optimization, higher-precision reference capability checks,
larger independently encoded photographic inputs, repeated final measurements,
and sanitizer fuzzing against the final source. The existing arithmetic,
lossless, hierarchical, metadata, and precision behavior must remain covered.
