# Song-pack decoder validation

The pinned [Club Fantastic song archive](https://clubfantastic.dance/) adds 32
real StepMania songs, including both Seasons 1 and 2, beyond the two shipped
ITGmania songs. Every audio file has a paired SSC chart whose MUSIC field resolves
to that archive member. The archive SHA-256 is
`79c73b5a4af53384cd8c46cc0aaecff33562335ca6c02016e86daca90627d895`.
Only the manifest and test results are redistributed; media stays under ignored
`target/deadlib-audio-reference/corpus/club-fantastic-season2`.

`validation/songpacks.json` pins archive size/hash and all 64 audio/chart members.
The 32 streams are stereo: 31 at 44.1 kHz and one at 48 kHz. Their vendor headers
identify libvorbis releases 20120203, 20150105 and 20200704. The separate upstream
and historical suites cover other rates, channel counts and older encoders.

All eight gates pass using unmodified libogg 1.3.6, libvorbis/libvorbisfile 1.3.7
and Symphonia 0.6.1:

| Gate | Checked material |
| --- | --- |
| Container | 39,616 pages and 465,274 packets against libogg |
| Headers/codebooks | 1,406 books, 138,492 entries; exact reference VQ values |
| Raw PCM | 393,205,888 channel samples per decoder, including priming/reset |
| Retained PCM | 393,176,540 channel samples per decoder; exact counts/timestamps |
| Streaming/reset | All 32 files, 17- and 4,093-byte chunks, exact native PCM bits and metadata after reset |
| Native seeks | 1,600 requests, exact positions/counts, 12,582,976 samples |
| Reference seeks | 544 requests, including repeated short reads and stream endpoints |

Raw and retained libvorbis peak error is at most `8.344650269e-7`; aggregate RMS
is below `7.224e-8`. Symphonia peak error is at most `5.960464478e-7`, RMS below
`4.245e-8`. Native seek peak error is below `7.749e-7`. The original numerical
bounds remain unchanged; counts and positions have zero tolerance. No shifting,
resampling, gain fitting or clipping is applied.

The additional repeated-block preroll test crosses 33/64 modes, both block
orders, all residue types and submaps in 48 streams. It checks 23 requests per
stream, including reads past the declared end where failed preroll exposes
retained reference PCM. Full raw overlap blocks receive their original separate
peak/RMS gates.

Reproduce after building the pinned oracles described in `tests/oracle/README.md`:

```powershell
python crates/deadlib-audio/decode/scripts/fetch_songpacks.py
python crates/deadlib-audio/decode/scripts/validate_songpacks.py --output target/songpack-validation-new
```

Python 3.9+ and curl are sufficient. `--offline` on the fetch script verifies an
existing archive and every selected member without downloading. Validation
requires a new output directory and retains commands, source/input/oracle hashes,
full logs and PCM/seek TSV tables. Set `DEADLIB_LIBOGG_ORACLE` and
`DEADLIB_LIBVORBIS_ORACLE` for non-default build paths. No performance is measured.
