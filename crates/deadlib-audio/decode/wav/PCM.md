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

The experiment establishes native sample/API parity within its fixture scope.
It does not establish full BWF/BW64 application-profile conformance, all metadata
semantics, physical large-file behavior, or representative performance. Wider
reference comparison with canonical padding and separate metadata profiles follows.
