# Setup combinations and packet recovery

This milestone follows the user's parity priority: exact behavior and counts,
with the original justified PCM tolerances. It corrects three packet/residue
differences and a numerical cancellation failure. It makes no complete-parity
claim; the remaining differences below stay explicit.

## Corrections

* An unused mode slot in a non-power-of-two mode table now discards the packet
  without advancing overlap or position, matching `vorbis_synthesis`. The
  low-level `Setup::read_audio_header` retains its invalid-mode diagnostic.
  The shared prefix helper also fixes Ogg streaming and index construction.
* A surplus classword from an oversized residue phrasebook now stops that
  residue, retains completed additions and leaves subsequent bits available.
  Pinned libvorbis intentionally accepts oversized books for old encoders;
  it terminates residue decoding when a decoded word exceeds the classification
  cardinality. Native synthesis previously rejected the entire packet.
* A residue with no active cascade stages consumes no classwords. Previously
  native decoding read phrasebook words even when every cascade was zero,
  displacing the next submap's data. The retained type-zero example had about
  `14.37` peak PCM error before this correction.

The last two rules track the reference's `res0.c` implementation. Section 8.6.2
of the specification expresses an unconditional eight-pass loop and base-N
classification digits; it does not express either reference boundary in that
form. These are explicit reference compatibility decisions, alongside the
earlier [type-zero partition commit rule](PARITY.md).

An eight-stage residue followed by a nearly silent overlap exposed a separate
accuracy failure: the f32 FFT produced approximately `7.87e-6` peak error where
the unchanged bound was approximately `7.79e-6`. Transform rotations, twiddles,
scratch, FFT arithmetic and analysis folds now use f64, with f32 output stores.
The independently derived factorization remains unchanged. This passes the
same per-block bounds; no comparison changes scale, shifts samples or fits gain.
Transform elements increase from `7N` to `14N` bytes. Decoder/encoder preflight
and storage accounting include that cost; repeated transforms still allocate
nothing. Historical speed/resource results describe their recorded revisions.

Sources: [Vorbis I sections 3, 4.3.1 and 8.6](https://xiph.org/vorbis/doc/Vorbis_I_spec.html),
[pinned packet synthesis](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/synthesis.c),
[pinned residue implementation](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/res0.c).

## Independent matrix

`tests/support/synthetic.rs` constructs non-silent headers and packets directly
with the bitpacker, independently of the native encoder. The scalar classbook
has eight entries; the VQ book returns one or three. Distinct channel gains make
misplaced bits/channel data visible. Each case resets synthesis, supplies two
pre-roll packets, a candidate packet and two recovery packets.

| Matrix | Setups | Cases | Packet outcomes | Discarded | PCM channel samples |
| --- | ---: | ---: | ---: | ---: | ---: |
| Mode tables | 128 | 26,486 | 132,430 | 16,086 | 15,059,264 |
| Residue combinations | 180 | 5,120 | 25,600 | 0 | 2,293,760 |
| Total | 308 | 31,606 | 158,030 | 16,086 | 17,353,024 |

The mode matrix covers counts 1 through 64, alternating block flags in both
orders, every valid/unused mode number, all four long-window flag pairs, byte
prefixes crossing the window boundary, empty packets and non-audio packets.
The residue matrix crosses all three types, phrasebook dimensions 1/2/3,
classification counts 1/2, stage masks `0`, `1`, `2`, `128`, `255`, and one
submap versus a stage-free first submap. It replaces each phrasebook word/vector
with each entry 0 through 7, covering surplus words, incomplete final dimension
groups and later-stage accumulation. Case totals include repeated zero-stage
control inputs, whose unused replacement parameters produce identical bytes.

Every reference block receives the original peak/RMS bounds (`3e-6` / `5e-7`,
scaled by `max(1, reference peak)`). Decisions, block sizes, frame counts and
raw positions must agree exactly. The matrix adds no nonfinite exclusions.
Existing floor-zero mutation tests retain their previously documented eleven
nonfinite-reference cases, outside finite PCM parity totals.

Two unit regressions check exact residue bit positions, non-exhaustion, following
sentinel bits and preservation of all untouched output/classification storage.
Another normal test checks invalid modes at the start and middle of strict Ogg
streams: one-byte/fragmented input, exact 320-frame output, and 104 repeated,
boundary/backwards seeks using both pre-roll strategies. Native suffix PCM is
bit-exact after seeking.

The 13,836-byte, eight-set `synthetic-setup-v1.libvorbis-pcm` golden retains C
responses for default tests, including all three fixes and the cancellation
case. Its inputs are procedural and redistributable. External regeneration is
manual; ordinary tests never write the golden.

## Validation and reproduction

Final-source validation passes **134 release tests** and **101 extracted offline
tests** (35 external/manual tests ignored in the extracted workspace), formatting,
default/all-feature Clippy, and the two fuzz-harness unit tests. Existing gates
include 38 repository assets, 528 upstream vectors, the 13-entry external corpus
manifest and 144 native encoding cases. The previous 7,014 prefix cases and
55,845 finite-reference bit cases are rechecked without changing their counts
or tolerances. The eleven nonfinite-reference cases still hit the finite-output
guard and remain excluded from finite PCM totals.

The first/middle/last case of every new setup plus all eight golden cases are
checked through **932 fresh-process C decodes**, byte for byte against the
reset-case protocol. The golden is independently recaptured and verified byte
for byte. Four 300-second AddressSanitizer campaigns (headers, packets, streaming,
encoding) complete **194,327 executions** without failure artifacts or slow
units. Sixteen targeted seeds add the procedural headers and strict Ogg versions
of the eight regressions. Seed granules come from C PCM counts. Fuzzing checks
bounded behavior/state consistency; it does not establish reference equivalence.

Pinned references remain libvorbis 1.3.7
`0657aee69dec8508a0011f47f3b69d7538e9d262` and libogg 1.3.6
`be05b13e98b048f0b5a0f5fa8ce514d56db5f822`. The C oracle is unchanged.
The [checks manifest](validation/2026-10-27-configuration-checks.json) records
current source/input/binary hashes and 36 retained artifacts. The
[reference probe](validation/2026-10-27-configuration-reference-probe.py) records
all generated request hashes, the fresh-process checks and the empty-book
discrepancy. Large derived requests and evolving fuzz corpora remain under
ignored `target/`; procedural builders and probes retain reproduction details.

With `DEADLIB_LIBVORBIS_ORACLE` configured as in the oracle README:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --test configuration_parity -- --include-ignored --skip regenerate_synthetic_setup_golden --nocapture
cargo test -p deadlib-audio-decode-ogg -p deadlib-audio-decode-vorbis --release
```

The [full validation probe](validation/2026-10-27-configuration-validation-probe.py)
records all external corpus/report environment variables and complete gates.

## Remaining scope

The reference accepts an appended, unreferenced sparse book with no used
entries and emits PCM identical to the baseline. Native strict parsing rejects
the empty Huffman tree. Section 3 rejects underpopulated trees and defines an
exception for one active entry, without an empty-tree exception. This audit
preserves that strict behavior and records the discrepancy outside parity totals.
No new permissive mode is implied.

The setup matrix uses two channels, uniform stage masks across classifications,
and a small set of residue geometries. Other channel/coupling/geometry, corpus,
streaming and seeking gates remain separate evidence. Neither this matrix nor
fuzzing proves full conformance. The [existing discrepancy ledger](COVERAGE.md)
and [nonfinite-reference cases](BIT_MUTATIONS.md) still apply. Application
integration and performance optimization remain outside this milestone.
