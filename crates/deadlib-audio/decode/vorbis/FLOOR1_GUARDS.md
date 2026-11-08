# Floor-one reconstruction, guards and EOF parity

The target is exact behavior, counts and positions with the original PCM
tolerances. This milestone adds explicit reference floor-one reconstruction
without replacing the default guard suggested by the published format.
It does not establish complete Ogg/Vorbis parity.

## Independent policies

`DecodeOptions::floor1_behavior` selects `Floor1Behavior::Vorbis` (default) or
`Floor1Behavior::Libvorbis`. The same field in `OggOptions` and `SeekOptions`
survives reset, index ownership and every selected decoder reconstruction.
Raw setup parsing does not choose the synthesis policy.

The [specification, section 7.2.4](https://xiph.org/vorbis/doc/Vorbis_I_spec.html)
recommends guarding reconstructed values after prediction. The default retains
that guard: clamp each Y to [0, range - 1], then multiply by the floor multiplier.
The specification permits guard variants for values produced by abused setup
and codebook combinations. Ordinary in-range data agrees under both policies.

The pinned reference's
[floor-one callbacks](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/floor1.c)
use the behavior below:

| Boundary | Explicit libvorbis policy |
| --- | --- |
| Prediction inputs | Mask each neighbor ordinate to 15 bits. |
| Nonzero wrapped difference | Reconstruct the Y value and mask it to 15 bits; activate and unmask its neighbors. |
| Rendered point | Multiply first, then clip to 0..255 before line interpolation. |
| Missing initial Y field | Preserve the signed read result -1 and sticky EOF. With no subsequent failing scalar read, the backend can still return a floor. |
| Truncated master/subclass word | Conceal this floor without introducing sticky EOF; preserve EOF that already existed. |
| Empty master/subclass book | Conceal this floor without consuming a word. |

For multiplier four, the default maximum is 63 times four, or 252. The reference
can reach 255 when an out-of-range reconstructed point is multiplied and clipped.
For large scalar differences, the reference's 15-bit wrap can also change later
neighbor predictions. Safe Rust integer arithmetic preserves those observed
results without relying on C signed-shift behavior outside nominal ranges.

A missing initial Y is distinct from an absent floor. If the flag was read as
one, both Y reads fail, and no scalar read follows, the reference returns its
minimum gain curve rather than an unused floor. The direct component checks
compare this presence decision and reader state, even when zero residue makes
the distinction inaudible in a complete packet.

Codebook interpretation, residue geometry, floor-zero behavior and nonfinite
PCM admission remain independent options. Selecting the floor-one policy does
not enable IEEE output. Reference packet comparisons explicitly select the
appropriate policies together; the default finite-output guard remains tested.

## Differential evidence

The test-only C command calls unmodified `floor1_inverse1` and `inverse2` through
the reference backend table. Curves begin with unity input; C and native front/
back guards must remain untouched. The independent wire builder uses sparse
scalar books with large original entry numbers, rather than a native encoder.

The direct matrix covers 264 setups, multipliers one through four, zero to
63 extra posts, range widths through 15, zero/one/three subclass bits, absent
subclasses, empty books and scalar values through 1,048,575. It tests both block
sizes, starting bit alignments, byte prefixes and preexisting EOF. All 83,876
presence/reader decisions and 6,710,080 gain values match exactly, including
66,840 present floors and 4,752 sticky-EOF cases. Reader positions agree before
the C reader enters its internal exhausted sentinel state; native public position
retains its documented last-successful-bit meaning. Sticky state matches in all
cases. An offline golden retains 146 cases across eight complete requests.

The full packet matrix covers 336 configurations, all multipliers, one/three/
eight channels, one/two/four submaps, all residue types, coupling, multiple floor
posts and large scalar values. It checks 15,243 sequences, 76,215 packet
decisions, 672 discards and 16,222,224 finite PCM values. Every block/frame count
and cumulative position agrees exactly; the normal peak/RMS bounds remain
3e-6 / 5e-7 times max(1, reference peak).

Six retained full-PCM regressions additionally check raw reset, pull decoding,
one-byte/17-byte/whole-input push/reset, and 336 native repeated boundary/backward
seeks across both floor-one policies and both pre-roll strategies. Native state
comparisons use complete PCM bits. The same stream bytes are decoded under both
policies, so the reference choice must survive each API boundary.

A fresh build of the preceding committed source has scaled peak/RMS errors of
0.0989738773 / 0.0056688052 on the retained three-channel witness. Explicit
reference reconstruction reduces them to 9.98257359e-8 / 8.68265587e-9. Both
builds return the same 368 frames and exact packet positions. The original
numerical bounds are unchanged.

## Reproduction

Set `DEADLIB_LIBVORBIS_ORACLE` to the pinned helper and run:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --lib floor::floor_one_tests -- --include-ignored --skip regenerate_floor1_component_golden --nocapture
cargo test -p deadlib-audio-decode-vorbis --release --test floor_one_guards -- --include-ignored --skip regenerate_floor1_pcm_golden --nocapture
```

Normal tests read the retained component and PCM fixtures. Manual regeneration
requires explicit ignored-test selection and the reference helper. Reference
sources remain libogg 1.3.6 at `be05b13e98b048f0b5a0f5fa8ce514d56db5f822` and
libvorbis 1.3.7 at `0657aee69dec8508a0011f47f3b69d7538e9d262`. Production depends
only on the native Ogg crate. No application integration or performance
measurement is part of this milestone. Wider parity work remains in the
[ledger](PARITY.md).

Final validation passes 190 release tests and 132 extracted offline tests
(71 external/manual tests ignored offline), formatting, default/all-feature
Clippy, two fuzz-library tests and fuzz Clippy. Three 300-second AddressSanitizer
campaigns complete 248,807 executions with no failure artifacts or slow units.
The 540 targeted seeds derive from 54 independently C-checked sequences.
Fuzzing checks bounded behavior and state consistency; it does not prove full
reference equivalence.

Fresh C processes recapture all 792 callback batches and both new goldens.
Another 8,070 isolated records cover each observed header/block/alignment/
presence/EOF and zero/unit/flat-curve category without preceding callback state.
Six C Ogg decodes and 84 repeated C seeks match raw synthesis exactly. Both
current and preceding helpers return identical new PCM goldens. The current
helper preserves 34 older PCM responses byte for byte; all 18,077 requests from
the preceding procedural matrices remain byte-identical.

[Checks and provenance](validation/2026-10-27-floor1-guards-checks.json) retain
commands, source/corpus hashes, counts, sanitizer evidence and artifact hashes.
[Independent C checks](validation/2026-10-27-floor1-guards-independent-reference.json)
record complete recaptures and unchanged requests. The
[compiled before/after witness](validation/2026-10-27-floor1-guards-before-after.json)
retains final-source, driver and executable hashes plus the original PCM bounds.
External audio and generated corpora remain in ignored storage.
