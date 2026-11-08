# Behavioral parity

The decoding goal is complete; the [final audit](DECODING_COMPLETION.md)
records the full scope and current gates. Packet decisions, counts, positions,
timestamps, channel order, streaming, resets and seeking require exact agreement
under each documented policy. Existing performance reports remain diagnostics.

The user confirmed the original decoder-to-reference PCM tolerances: normal
peak/RMS bounds are `3e-6` / `5e-7`, scaled by `max(1, reference peak)`;
historical floor-zero bounds remain `5e-5` / `3e-6`. Counts and behavior receive
no tolerance. This compares decoders reading identical encoded data, separately
from a lossy encoder's source-reconstruction error.

The subsequent [bit-mutation matrix](BIT_MUTATIONS.md) fixes floor-zero selector/
rounding and VQ sequence-offset differences. It adds 55,845 finite-reference
cases and explicitly separates eleven cases where libvorbis emits nonfinite
PCM. Its final validation passes 128 release / 97 offline tests.

The subsequent [configuration matrix](CONFIGURATION_PARITY.md) covers all mode
counts/block orders and additional residue setup combinations. It fixes unused
mode discard, surplus classword termination and stage-free submap consumption,
and retains the original PCM bounds through wider transform intermediates.

The [geometry follow-up](RESIDUE_GEOMETRY.md) checks mixed stage books, sparse
channel participation and multiple active submaps through 255 channels. It adds
an explicit libvorbis layout for type-two ranges whose endpoints are not channel
aligned, alongside the published interleave rule.

The [degenerate-book follow-up](DEGENERATE_BOOKS.md) adds empty-book consumer
behavior and explicit IEEE propagation through raw/Ogg/seek APIs. All eleven
previously recorded nonfinite cases match under that policy; the default finite
guard remains separately tested. Nonempty zero-dimensional scalar books are
also accepted where vector reconstruction is unnecessary.

The [codebook extent follow-up](CODEBOOK_LIMITS.md) applies the default reference
declaration guard before lengths/allocation for populated and empty books, with
475 exact decision/reader-position checks and a configurable full-field override.

The [residue range/tail follow-up](RESIDUE_TAILS.md) adds an explicit reference
policy for partial VQ partitions and inverted empty ranges. It preserves reference
codeword counts, clipped additions and reader EOF state across active submaps.
The published whole-vector and ordered-range guards remain default.

## Corrected discard and truncation behavior

The 2026-10-27 work independently reproduced two differences against pinned
libvorbis 1.3.7:

* Discarding a packet can invalidate its neighbors' encoded window hints.
  Previously, native synthesis windowed the previous right half before knowing
  the next accepted block size, and chose the next left window from its hint.
  A stereo loss case had about `0.1444` peak PCM error. Synthesis now retains
  unwindowed right-hand data and windows the actual adjacent accepted blocks,
  matching `vorbis_synthesis_blockin`. Deferred weighting reads saved overlap
  without mutating it, preserving recovery after a failed decode.
* `vorbis_book_decodevs_add` gathers a complete type-zero residue partition's
  codewords before committing additions. Native synthesis previously kept
  partial additions from an unfinished partition. It now preflights a cloned
  bit reader, preserving completed partitions/passes and copying back consumed/
  exhausted state on failure, without additional packet allocation. A legacy
  prefix exceeded the existing RMS bound before this fix; the fixed suite's
  floor-zero peak/RMS errors are below `9e-9` / `1e-9`.

The specification treats residue end-of-packet as nominal and expresses
type-zero decoding per vector. Complete-partition commit follows libvorbis's
implementation for that boundary; keep this reference-compatibility detail
explicit when reviewing specification behavior.
Sources: [Vorbis I §8.6](https://xiph.org/vorbis/doc/Vorbis_I_spec.html),
[pinned block synthesis](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/block.c),
[pinned codebook decode](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/codebook.c).
No numerical tolerance, Ogg validation rule or legacy granule policy changed.

## Independent evidence

Each case resets both decoders, retains up to two original pre-roll packets,
replaces one packet with every byte prefix including empty and complete, then
supplies up to two original following packets. Generated fixtures cover every
audio packet. The historical file covers six initial packets plus eight
additional packets selected for nonzero PCM.

| Input | Prefix cases | Packet decisions | PCM channel samples |
| --- | ---: | ---: | ---: |
| Mono | 67 | 232 | 41,216 |
| Stereo | 890 | 3,384 | 4,869,632 |
| Eight-channel silence | 9 | 27 | 94,208 |
| Eight-channel dependent coupling | 3,067 | 11,171 | 62,770,176 |
| Historical floor zero | 2,981 | 14,892 | 15,297,536 |
| Total | 7,014 | 29,706 | 83,072,768 |

Discard/PCM decisions, block sizes, frames and raw positions are exact; each
output block's PCM receives the unchanged error checks. Both floors and all
three residue types are represented. The new `pcm-cases` oracle uses
`vorbis_synthesis_restart`; 21 fresh-process `pcm` runs verify its output byte
for byte. The existing `pcm` mode is retained.

The 29,234-byte `discarded-window-v1.libvorbis-pcm` golden makes empty/non-audio
recovery a normal test without C tools. It captures the generated stereo fixture
with raw packet index four replaced by an empty packet. The residue-zero unit
regression checks completed/incomplete partitions, exact consumption and all
plane/sentinel values.

Validation passes **123 release tests**, **94 extracted offline tests** (30
external/manual tests ignored there), formatting and default/all-feature
Clippy. Existing gates include 38 repository assets, 528 upstream coded vectors,
the pinned historical/legacy corpus and 144 native encoding cases. Two five-
minute AddressSanitizer campaigns complete **12,810 executions** without failure
artifacts. Fuzzing checks bounded behavior/state consistency, not complete
reference equivalence.

```powershell
$env:DEADLIB_LIBVORBIS_ORACLE = (Resolve-Path target/deadlib-audio-reference/vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe).Path
$env:DEADLIB_VORBIS_FLOOR0 = (Resolve-Path target/deadlib-audio-reference/corpus/floor0/vorbis_floor_type_0.ogg).Path
cargo test -p deadlib-audio-decode-vorbis --release --test concealment -- --include-ignored --test-threads=1 --nocapture
```

[Checks/provenance](validation/2026-10-27-concealment-checks.json) retain hashes,
before/after failures, commands, reports and sanitizer evidence.
[Golden provenance](validation/2026-10-27-concealment-golden-capture.json) and
[case-runner verification](validation/2026-10-27-concealment-oracle-selfcheck.json)
record the independent reference checks. External audio stays in ignored storage.

## Completion audit

The subsequent component/packet matrices, reference EOS/seeking work, invalid
granule rejection checks, valid untimed EOS handling and expanded 32-song corpus
resolve this ledger's earlier follow-ups. The
[final audit](DECODING_COMPLETION.md) reviews the named acceptance/count
differences individually and retains original PCM tolerances. Additional
conformance inputs and longer fuzz campaigns remain maintenance; sparse seek
strategies, performance, competitive encoder quality and application integration
are separate future tasks. No exhaustive all-bitstream certification is claimed.

The [floor-zero parameter follow-up](FLOOR0_PARAMETERS.md) records amplitude-width,
Bark-map and incomplete-LSP reader differences, plus the IEEE transform-class
regression. The default unsigned field policy and explicit reference policy are
checked separately; the original finite PCM tolerances are unchanged.

The [codebook values and bounds follow-up](CODEBOOK_VALUES.md) separates the
published custom-float range and exact bit availability from libvorbis's exponent
clamp and whole-byte checks. Reference values, guard decisions/positions and
initialization are independently checked before packet and Ogg comparisons.

The [floor-one guard follow-up](FLOOR1_GUARDS.md) adds
`Floor1Behavior::Libvorbis` for reference 15-bit reconstruction masks, rendering
clips and EOF decisions. The original range guard remains default. The policy
is retained through raw reset, streaming and indexed decoder reconstruction;
finite PCM still uses the original numerical bounds.
