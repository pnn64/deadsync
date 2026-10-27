# Codebook values and array-bound parity

The target is exact behavior, counts, positions and channel order with the
original PCM tolerances. This milestone adds an explicit codebook reference
policy while preserving the published format equations and bit bounds by default.
It does not establish complete Ogg/Vorbis parity.

## Published and reference policies

`ParseOptions::codebook_behavior` chooses `CodebookBehavior::Vorbis` (default)
or `CodebookBehavior::Libvorbis`. The same field in `OggOptions` and `SeekOptions`
is retained through streaming resets, index construction and every selected
setup reparse. This affects setup reconstruction; a raw `PacketDecoder` receives
the already reconstructed `Setup`.

| Boundary | Published/default policy | Explicit libvorbis 1.3.7 policy |
| --- | --- | --- |
| Custom-float exponent | Full mantissa ? 2^(field ? 788) equation | Clamp the unbiased exponent to ?63?63 |
| Unordered length array | Exact available bits | Rounded required bytes versus whole remaining bytes |
| Quantized lookup array | Exact available bits | Rounded required bytes versus whole remaining bytes |

The published [helper equation, section 9.2.2](https://xiph.org/vorbis/doc/Vorbis_I_spec.html)
has no exponent clamp. The pinned reference's
[`_float32_unpack`](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/sharedbook.c)
clamps before scaling the signed 21-bit mantissa. Exponent fields 725?851 agree;
values outside that interval differ. For example, field 724 and mantissa one
produce 2^-64 by the published equation and 2^-63 in the reference. Field 852
produces 2^64 versus 2^63. At field 1023 the native published VQ conversion can
become infinity, while the clamped reference value remains finite.

`codebook::float32_unpack` retains the full published range in f64.
`float32_unpack_with_behavior` offers both interpretations. Clamped unpacked
values are exactly representable in binary32. VQ reconstruction preserves the
reference's double expression and f32 sequence carry, including cancellation.

The byte guards follow
[`vorbis_staticbook_unpack`](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/codebook.c).
A complete ten-byte scalar book can contain every coded bit yet fail its length
array guard: the native default consumes 80 bits; the reference rejects at bit
66. A mapped example contains its last quantized bit at bit 159, but the reference
rejects there because no whole byte remains. Appending a byte permits both.
The reference policy retains those exact guard positions and leaves logical EOF
unset for an early guard rejection. Its unordered-length preflight runs before
length allocation. Successful preflight leaves the shared parser at its original
position. Quantized-array byte arithmetic uses u64 to remain safe when callers
raise independent resource caps on a 32-bit host.

Empty-book acceptance, extent/entry/value/memory caps, IEEE output and floor-zero
amplitude behavior remain independent policies. Selecting this codebook policy
does not enable those options automatically.

## Independent evidence

The C helper calls unmodified private unpack, book-initialization and custom-float
functions. The float grid contains 20,480 words: every one of 1,024 exponent
fields, both signs and ten zero/edge mantissas. Returned binary32 bits match
exactly. A second oracle constructs IEEE fields from integer-mantissa
normalization without evaluating the implementation's power expression.

The bound grid contains 83,968 complete, valid-tree books across all eight
starting alignments, entry counts 1?64, dimensions 0/1/2/3/8 where the mapping is
defined, lookup types 0/1/2, 1/2/8/16-bit quantization, sparse/dense lengths and
padded/unpadded tails. The native published policy accepts every complete bit
sequence. The reference policy matches 77,048 accepted and successfully
initialized books, 6,920 guard rejections and every consumed-bit position. The
grid avoids nonempty zero-dimensional lookup-one VQ contexts.

Another 2,208 books cover both VQ lookup types, sparse/dense entries, sequence
carry, cancellation, all relevant exponent boundaries, signs, quantizer widths
and dimensions through 65,535. Every used vector value and emitted scalar
codeword matches the independent C response: 6,338,688 exact vector values.
Component buffers include explicit trailing padding where the private reference
reader requires whole bytes; the bound grid separately retains unpadded cases.

The packet matrix covers 420 configurations, both floors, all residue types,
multiple active submaps, coupling and 1/2/3/8/17 channels. It checks 23,460 cases,
117,300 packet decisions and 840 discards, followed by recovery. All block/frame
counts and cumulative positions agree exactly. Its 25,894,160 finite PCM values
use the original normal peak/RMS bounds 3e-6 / 5e-7, or floor-zero bounds
5e-5 / 3e-6, each scaled by max(1, reference peak). Another 1,328,400 nonfinite
values match per-position NaN classes and infinity signs. NaN payloads are
unspecified.

Six retained PCM cases exercise the policy through raw reset, pull decoding,
one-byte/17-byte/whole-input push/reset, and 336 native seeks across both
codebook policies and both index pre-roll strategies. Native comparisons use
all PCM bits after canonicalizing NaN payloads. A maximum-exponent witness
asserts that published and reference output differ, so silently dropping the
option cannot pass the state-consistency checks.

## Reproduction

Set `DEADLIB_LIBVORBIS_ORACLE` to the pinned helper and run:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --test codebook_floats -- --include-ignored --skip regenerate_codebook_float_golden --nocapture
cargo test -p deadlib-audio-decode-vorbis --release --test codebook_float_packets -- --include-ignored --skip regenerate_codebook_float_pcm_golden --nocapture
```

Normal tests only read the retained fixtures. Explicit manual generators are
`regenerate_codebook_float_golden` and `regenerate_codebook_float_pcm_golden`,
each requiring `--ignored` and the reference helper. The packet inputs patch
known custom-float fields in independent procedural headers; the native encoder
does not generate the comparison data.

Reference sources remain libogg 1.3.6 at
`be05b13e98b048f0b5a0f5fa8ce514d56db5f822` and libvorbis 1.3.7 at
`0657aee69dec8508a0011f47f3b69d7538e9d262`. Production depends only on the native
Ogg crate. No application integration or performance measurement is part of
this milestone. The [parity ledger](PARITY.md) retains the wider unfinished work.

The finite before/after witness keeps exactly 368 frames (736 channel values).
At exponent field 852 and mantissa one, the preceding codec milestone
`b5953ce15` with published defaults has scaled peak/RMS errors about 1.0 / 0.09992
against the pinned clamped reference. Explicit reference parsing reduces those
to 1.665439061e-7 / 2.332770493e-8, within the original bounds for every block.
The old and new C helpers return identical PCM for this witness. Published
semantics remain separately tested and unchanged.

Fresh-process validation cross-checks the new unpack mode with the existing
book/unquantize mode at every exponent/sign (2,048 cases), checks 1,016 selected
bound records independently of batch state, and recaptures all 2,208 VQ matrix
responses. Both retained goldens recapture byte for byte. Twenty-eight old/new
C PCM responses are identical, including the six new packet cases. Six C Ogg
decodes and 84 repeated C seeks match their raw PCM suffixes after NaN payload
canonicalization.

Final gates pass: 175 release tests with the reference/corpus checks, 123 normal
tests in an independent offline workspace (61 external/manual tests ignored),
default/all-feature Clippy and two fuzz-harness unit tests plus lint. Earlier
matrices retain their exact scope and original tolerances, including
1,836,059,120 residue-geometry PCM comparisons. The diagnostic decoder copy is
generated and linted only; no timing run is performed.

Three 300-second AddressSanitizer campaigns (headers, raw packets and streaming)
complete 280,752 executions with no failure artifacts or slow units. They inherit
the preceding corpus and add 716 targeted seeds, including 70 independently
C-verified initialization/PCM/count probes and both sides of the byte guards.
Source and binary hashes are rechecked against the tested inputs.

The [checks/provenance](validation/2026-10-27-codebook-floats-checks.json),
[validation probe](validation/2026-10-27-codebook-floats-validation-probe.py),
[reference probe](validation/2026-10-27-codebook-floats-reference-probe.py),
[reference results](validation/2026-10-27-codebook-floats-independent-reference.json)
and [before/after evidence](validation/2026-10-27-codebook-floats-before-after.json)
retain commands, original failures, small procedural witnesses, golden hashes,
corpus reports and sanitizer evidence. The original codec snapshot is verified
against `b5953ce15`. Complete parity remains unfinished.
