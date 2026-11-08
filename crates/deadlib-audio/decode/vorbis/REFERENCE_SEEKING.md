# Reference file seeking

`SeekOptions::seek_behavior = SeekBehavior::Libvorbis` uses native raw synthesis
queues to reproduce libvorbisfile 1.3.7 seeking. This closes the previously
recorded 172 EOS seek-position/count differences in the explicit reference mode.
The default `SeekBehavior::Retained` continues to return exact suffixes of a
sequential decode. Its original inventory remains asserted separately.

The reference scheduler selects the last page landmark strictly before the
requested granule, numbers packets from the reset stream queue, performs
track-only preroll, consumes queued PCM to the requested position, and preserves
the synthesis block size and backing storage across repeated seeks. A continued
landmark with no available packet triggers backward page selection and the
reference raw-seek two-queue scan. Native indexed pages supply these decisions;
no C library or external process is used in production.

Failed track-only window prefixes also need the previous block object's timing
and PCM pointers: vorbisfile still inserts that block after the failure. The
reference mode retains a bounded time arena, including the overlaid short-block
layout. A 33-mode/long-first regression previously differed by about 35.16 peak
PCM after repeated seeks. The corrected mode matches C, while the public raw
`track()` API still discards rejected prefixes transactionally. The arena is
charged to decode and selected-stream memory limits.

The first-page rule deliberately uses the page observed at the end of the
landmark search. When a priming packet spans pages, that can discard its first
fragment and reproduce libvorbisfile's lost first block on a PCM seek to zero.
Broadcast offsets likewise reproduce the reference's reported position and
sample sequence instead of silently converting them to retained-frame slices.

Codec acceptance/numerical policies and container compatibility remain explicit
separate options. For the reference matrices, select libvorbis codebook,
residue, floor-zero/one, residue-two layout and EOS behavior; enable empty books
and IEEE propagation where required. Reference seeking cannot be combined with
the indexed cross-packet endpoint repair. CRC, sequence, resource and framing
checks remain in force. Stream selection remains per logical stream; this API
does not automatically read across a chain boundary or repair arbitrary files.

Both reference read methods return the next nonempty synthesis queue, skipping
priming/discarded packets and any intervening pages. Metadata/start/end events
remain per selected stream. `position()` and PCM start positions follow reference
clock updates and may differ from a sequential PCM suffix on malformed files.
`StreamEnd` still describes the complete sequential stream metadata. Packet
scratch is charged to the selected-stream memory budget; synthesis memory uses
the larger queue-aware requirement. Invalid seek arguments preserve the cursor;
IO failures are terminal until a successful seek.

Validation includes exact status/position/count checks and the original finite
PCM bounds (peak 3e-6, RMS 5e-7 times max(1, reference peak)). The EOS matrix checks
all 4,476 requests across 1,260 streams against fresh C responses, including all
172 former disagreements. Retained C captures give offline regressions for
EOS case 115, positive broadcast offset 900 and failed first-long preroll. Additional direct comparisons
cover repeated short/full reads, the 394-KiB spanning-prime fixture, mixed-rate
chains, a bounded 70,000-byte padded priming packet, all 528 pinned upstream
rate/channel/quality vectors, repository assets, historical floor-zero media,
aoTuV surround and the two pinned ITGmania songs. Resource limits, fragmented IO, invalid arguments and
failure recovery have offline tests. Fuzzing pairs readers with identical seek
history rather than assuming restarted reference storage is fresh.

A further 192 procedural streams cross mode counts 1/2/3/5/17/33/63/64, both
block orders, every residue type, submaps and truncated first/intermediate
preroll packets. Each complete raw overlap block independently receives the
original peak/RMS checks. File-seek PCM uses the existing seek suite's aggregate
criterion across all returned samples, including one-frame reads; it does not
renormalize the block error budget to a near-zero one-frame crop. Counts and
positions remain exact for every request. No numerical bounds were widened.

The subsequent 48-stream repeated-block matrix checks pending/failed preroll
with reads past the declared end. A complete 32-song pack adds 544 repeated
reference requests. [Untimed EOS](UNTIMED_EOS.md) checks readable PCM despite
zero reference duration; completed audio pages with negative/unknown granules
remain rejected in every policy. The [final audit](DECODING_COMPLETION.md)
closes the parent decoding goal. Arbitrary corrupt-file recovery equivalence
and exhaustive conformance are not claimed. Performance is excluded.
Validation commands and hashes remain under `validation/`.
