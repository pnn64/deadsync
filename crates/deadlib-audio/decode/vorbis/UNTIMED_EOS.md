# Empty EOS pages without a timestamp

An empty Ogg EOS page may carry the unknown granule sentinel (`-1`): no packet
finishes on that page. The container already accepted this framing, but the
Vorbis mapping previously rejected it after emitting the preceding PCM.

The default mapping now inherits the last validated completed-audio landmark.
An empty EOS introduces neither samples nor padding. Streaming, pull decoding
and indexed reads use the same effective endpoint. `StreamEnd.end_granule`
reports that endpoint; the container page and underlying index retain the
original wire value.

The explicit `EosBehavior::Libvorbis` policy preserves a different duration
decision in libvorbisfile 1.3.7. Its seekable end scan clamps a negative final
granule to zero and clamps the resulting duration to zero. Native reference
metadata therefore reports zero declared frames while retaining the emitted
PCM count. Its effective endpoint equals the declared start. On the retained
stereo fixture, both decoders emit 4,672 frames despite that zero declared
duration. Actual unseekable C decoding and repeated C seek/read calls check the
audio; the duration-only helper's zero-total shortcut is not PCM evidence.

This acceptance applies to empty EOS pages. Completed audio pages carrying
unknown or other negative granules remain errors under every EOS/seek policy.
The regression places `-1`, `-2` and the minimum signed value on first, middle
and final completed-audio pages, across 90 API/policy paths and 18 reset checks.
CRC, sequence and packet validation are unchanged.
Negative encoded landmarks are distinct from an inferred negative initial
offset: valid nonnegative landmarks can still describe initial sample trimming.

`tests/nil_eos.rs` covers empty, priming-only and multi-packet streams, one-byte
through 4,093-byte fragmented input, reset replay, indexed suffixes and both EOS
policies. `tests/reference_seeking.rs` compares repeated short/full reads at
zero declared duration against actual libvorbisfile responses. All finite PCM
comparisons retain the original bounds. The fixture contains generated tones,
repacketized with one audio packet per page and an appended empty EOS page;
it contains no external song media.

The framing rule is in [RFC 3533 section 6](https://www.rfc-editor.org/rfc/rfc3533#section-6).
Reference duration behavior is checked against the pinned `vorbisfile.c`
`_bisect_forward_serialno` and `_open_seekable2` implementations. See
[the oracle instructions](tests/oracle/README.md) for source commits and builds.

[Focused checks and source hashes](validation/2026-10-28-untimed-eos-checks.json)
retain the successful boundary regressions and the completed post-fix song-pack
run. The final completion receipt records the broader current-source gates.
