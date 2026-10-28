# Native PCM and IEEE-float validation

The reference protocol uses signed i32 little-endian for integers, with the
container-scale value shifted to bit 31. This is an explicit comparison adapter;
production APIs retain native container scale. Float32/64 outputs contain the
original IEEE words in little-endian order, without widening, narrowing,
normalization or NaN comparisons. Float fixtures are generated from integer bit
words, including signed zeros, subnormals, infinities, quiet/signaling NaN payloads
and arbitrary seeded patterns. Integer fixtures include extrema and reduced-valid-
bit layouts whose unused low bits are zero.

`tests/pcm_fixtures.py` independently packs the RIFF structures and sample truth.
It does not call a production writer or a reference codec. The Rust validation
example exercises borrowed, seekable and Read-only frame APIs, exact ranges and
both typed writer finalization modes. The encoders must preserve sample bytes,
valid bits, masks and ancillary payloads; their output is decoded again and the
two encoded forms must agree. Reference builds remain outside production Cargo
edges: pinned dr_wav, libsndfile through SoundFile, and Symphonia 0.6.1.

## Initial exploratory matrix

`validation/pcm-exploratory.*` retains 380 fixtures, their complete ZIP and source/
binary hashes. All 1888 applicable native decode/range checks and 760 typed
encoder cases pass independent bit truth. The 24 mono/stereo nonempty RIFF cases
also match all three references. Exact whole-fixture comparisons are dr_wav
216/380, libsndfile 180/380 and Symphonia 72/380; errors and mismatches are retained.

This initial corpus uses nonzero odd padding, so it is compatibility evidence,
not a strict RIFF-writing conformance corpus. Reviewing the
[IBM/Microsoft RIFF specification, section 2-2](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/riffmci.pdf)
confirmed that canonical pad bytes are zero. Production writers already emit
zero padding. The next corpus revision separates canonical padding from explicit
noncanonical compatibility cases.

The exploratory results also expose reference coverage limits. The pinned
Symphonia debug build does not probe RIFX/RF64/BW64 and panics in channel-mask
arithmetic on 33-channel input. dr_wav does not accept BW64 here and reports a
zero translated subformat on these RIFX extensible fixtures; libsndfile matches
the RIFX extensible sample bits. libsndfile rejects RF64 with odd JUNK payloads
in this corpus. Reducing the latter to a zero-padded three-byte JUNK still
reproduces the failure, while bare RF64 and a four-byte JUNK decode exactly.
This distinguishes that behavior from the nonzero-pad issue.

## Canonical and compatibility matrix

`validation/pcm-canonical.{json,build.log,corpus.zip}` records the corrected
corpus: 440 canonical fixtures and 20 explicit compatibility fixtures (12 with
data before fmt and eight with nonzero odd padding). Canonical odd metadata uses
zero padding; the even-metadata profile is also retained to distinguish parsing
differences. The 460 fixtures cover RIFF, RIFX, RF64 and BW64; all six native
formats; 1/2/6/33 channels; empty, 17-frame and 521-frame audio; channel masks;
3-in-8, 12-in-16, 20/23-in-24, 24/1-in-32 valid-bit layouts; and non-data ds64
size-table entries. Fixture truth follows the
[Microsoft valid-bit/container rules](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ksmedia/ns-ksmedia-waveformatextensible)
and the [RIFF byte-order and chunk rules](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/riffmci.pdf).

All **460 fixture gates pass**, including 2748 applicable descriptor, complete
decode and range checks, plus 920 typed encoder cases. Forward-only input is
excluded from the 12 data-before-fmt cases according to its documented contract.
Both encoder modes preserve raw native bytes, valid bits, masks and ancillary
payloads, emit identical forms, and decode back to independent bit truth.
The 24 predeclared nonempty full-precision mono/stereo RIFF gates also pass all
four references. No other reference rejection is silently removed from the report.

| Reference, pinned version | Exact sample output / 460 |
| --- | ---: |
| dr_wav, a466dbe88fe2576d46c43877d602f3d66b8561a7 | 270 |
| libsndfile 1.2.2 / SoundFile 0.13.1 | 310 |
| Symphonia 0.6.1, debug reference build | 92 |
| FFmpeg 7.1 | 302 |

Every successful dr_wav, libsndfile and Symphonia decode is bit exact. Their
remaining cases return errors or, for the pinned Symphonia debug build's
33-channel mask arithmetic, panic. These counts describe this corpus and these
drivers; they are not library-wide conformance scores. dr_wav rejects BW64,
non-data sentinel size-table cases and these RIFX extensible GUIDs. libsndfile
does decode RIFX extensible valid-bit/mask fixtures exactly, independently checking
that byte-order path. Symphonia does not probe RIFX/RF64/BW64 in this build.
FFmpeg supplies an additional independent BW64 reader: 104/114 BW64 cases match.

The six tiny fixtures in `validation/pcm-rf64-reductions.{json,corpus.zip}` isolate
libsndfile's RF64 odd-JUNK behavior: bare RF64, even JUNK and INFO succeed; odd
JUNK before or after fmt fails with both zero and nonzero padding. The pinned
[libsndfile RF64 parser](https://github.com/libsndfile/libsndfile/blob/1.2.2/src/rf64.c#L317)
skips the declared JUNK length without its odd pad, consistent with this result.
That source explanation is an inference; the exact failures and bytes are retained.

FFmpeg has 62 successful but nonexact outputs: 23 empty RIFF/RIFX data cases emit
trailing metadata as samples; 21 legacy RIFX integer cases decode with little-endian
interpretation; nine 24-valid-in-32 cases select `pcm_f24le`; and nine 1-valid-in-32
cases select `pcm_u8`. `validation/pcm-ffprobe-descriptors.json` retains three
small input-hashed probes of those codec selections. The pinned
[FFmpeg format parser](https://github.com/FFmpeg/FFmpeg/blob/n7.1/libavformat/riffdec.c#L57)
uses the extensible valid-bit field during codec selection. The report retains
all sample-count/bit differences rather than adjusting comparison output to match.

The first canonical run incorrectly required Symphonia's optional sample-width
metadata for legacy float files. Those sample bits already matched, but eight
core metadata checks failed because the width is implicit in the codec.
`pcm-canonical-adapter-initial.{json,build.log}` and its hash-verified source ZIP
retain that failed adapter version. The corrected adapter emits JSON null for
an absent width and checks the field accordingly. No production codec change
was needed. Both runs use the same archived fixture bytes.

Reproduce from the workspace root (reference tools and versions above required):

```sh
python crates/deadlib-audio/decode/wav/tests/reference_pcm.py --output target/wav/new-pcm-run.json
python crates/deadlib-audio/decode/wav/tests/reduce_pcm_rf64.py --output target/wav/new-rf64-reductions.json
```

The runner builds validation-only drivers, fingerprints sources/binaries and
archives every fixture. It refuses to overwrite an earlier report. Source inputs
were unchanged during the retained final run. This establishes native sample/API
parity within the matrix; BWF/BW64 application-profile certification, complete
metadata semantics and representative performance remain outside its scope.
Physical large-file and working-set evidence is recorded separately in
[LARGE_FILES.md](LARGE_FILES.md).

After native wavl/slnt support and the explicit Option-based contiguous accessors,
`validation/wavl-pcm-regression.{json,build.log}` repeats the full 460-fixture
matrix. All 2748 descriptor/decode/range checks, 920 ordinary typed writer cases
and 24 four-reference core gates still pass, with the same reference exact counts.
The corpus hash matches the existing `pcm-canonical.corpus.zip`; no duplicate
archive is needed. WAVL.md records the separate segmented fixtures and writers.
