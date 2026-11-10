# Metadata coverage and preservation

Metadata interpretation is explicit: `Chunk::metadata(endian, limits)` builds a
borrowed view. `Wave::parse` and `Reader` continue scanning top-level headers
without reading/interpreting metadata payloads. Unknown chunks retain ID, raw
payload, original padding and offset. `Metadata::raw` returns the original bytes
for known types too. Text remains bytes; legacy code pages are not guessed.

Implemented views and caller-buffer writers:

- LIST, including INFO and adtl: bounded subchunk iteration, original pad bytes,
  byte strings, labl/note cue links and ltxt region/locale fields.
- cue: every cue record field, including chunk/block/sample offsets.
- smpl: sampler header, loop records, fractional positions and vendor-specific data.
- bext: BWF versions 0/1/2, 64-bit time reference, UMID, integer loudness, reserved
  bytes and coding history. Views retain invalid/unknown values; semantic loudness
  access ignores invalid/unset values. The writer validates ASCII text, date/time,
  loudness ranges and version-specific fields, and emits zero reserved bytes.
- r64m: 320-byte RF64 marker rows, flags, 64-bit sample/byte positions, labels,
  vendor GUID and user words. Invalid entries and unknown flags remain available.
- chna: every audio-ID slot, including unused reserved slots, repeated track
  assignments and both AT/AC references. Structural parsing checks slot/count
  consistency. `validate_assignments(channels, bitmap)` additionally checks ID
  syntax, track bounds and distinct used-track count without heap allocation.
- axml bytes, bxml compression envelope, and sxml compression/subchunk/alignment
  envelopes. Packed odd-length subchunk payloads and 64-bit sizes/positions are
  retained. Compression method and XML payloads are passed through unchanged.

Use each `write_*` helper (or `BextSpec::write`) to fill caller storage, then pass
that payload as a `RawChunk` to Writer/Muxer. Parsing and writers allocate no heap;
CHNA assignment validation uses a caller-provided bitmap of ceil(channels/8)
bytes, at most 8192. Serialization errors found during argument/capacity validation
leave the output untouched. Copy raw payloads to preserve noncanonical reserved
bytes and padding rather than re-encoding a canonical metadata structure.

Primary format references consulted 2026-10-28:

- [Microsoft RIFF update, 1994](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/RIFFNEW.pdf), cue, associated data and sampler chunks.
- [EBU Tech 3285 v2](https://tech.ebu.ch/docs/tech/tech3285.pdf), broadcast extension layout and loudness representation.
- [EBU Tech 3306 v1.1](https://tech.ebu.ch/files/live/sites/tech/files/shared/tech/tech3306v1_1.pdf), RF64 marker rows.
- [ITU-R BS.2088-2, 2025](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.2088-2-202511-I!!PDF-E.pdf), BW64 channel assignments and XML envelopes.

## Boundaries and remaining work

These APIs handle container metadata, not ADM rendering, XML schema validation,
gzip decompression, global UID uniqueness, sampler playback or cross-file reference
resolution. CHNA alignment to the audio format is an explicit validation call.
SXML alignment offsets are exposed and can be looked up with `subchunk_at`; full
alignment/timestamp semantics are not certified. Broadcast metadata views use
little-endian field layouts; dispatch rejects unsupported broadcast interpretation
inside RIFX while raw payloads remain accessible.
