# Physical large-file and process-memory evidence

`validation/physical-sparse.json` records eight real NTFS sparse files and 24
successful fresh-process checks on Windows 11, rustc 1.99.1. RF64 and BW64 are each
tested with native mono i32 and IMA ADPCM, at 65573 and 4294967333 decoded frames.
Each file is created/checked by an x64 release process, reopened by a separate x64
process, and reopened by an i686 process. Python independently checks marker
bytes, a zero hole, physical EOF and header hashes. Source and executable hashes,
compiler/build commands and build logs accompany the report.

| File geometry | Logical file length, bytes | NTFS allocated bytes |
| --- | ---: | ---: |
| Small native PCM | 262396 | 131072 |
| Small IMA | 262388 | 131072 |
| Large native PCM | 17179869436 | 262144 |
| Large IMA | 17179869428 | 262144 |

Both containers produce the same lengths for each geometry. The large files
exceed 16 GiB, and their frame counts exceed u32. Markers straddle the physical
4 GiB byte boundary and the 2^32 frame boundary, with additional start/end and
near-boundary samples. Marker values mix the high frame bits so truncating an
offset to 32 bits cannot return a matching sample. Exact EOF seeks and reads,
out-of-range rejection without changing position, and an unwritten zero hole
are checked through `Reader` and `AdpcmReader`.

This fixture builder uses known-length writer headers and then creates sparse
extents with the filesystem, so it does not claim that a writer accepted missing
audio writes. IMA uses valid header-only, one-frame blocks; this exercises block
index and decoded-count arithmetic without representing typical compression
geometry. The sparse flag, allocated size and file length are queried from the
actual file handle. The Windows
[sparse-file control](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_set_sparse)
is confined to the validation example; the production crate remains portable.

## Working-set scope

The validation process queries
[PROCESS_MEMORY_COUNTERS_EX](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
at startup, before decode and after decode. The table uses peak working set from
the separate verify processes: four fresh processes per architecture/size cell
(two containers times two codecs). Exact byte counts, current working set and
private/peak commit are retained in the JSON.

| Verify process | Small-file peak working set, MiB | Large-file peak working set, MiB |
| --- | ---: | ---: |
| x64 | 3.41–3.42 | 3.41–3.42 |
| i686 | 4.57–4.95 | 4.57–4.59 |

The measurements show bounded whole-process memory for these open/random-seek
probes as logical audio grows from about 256 KiB to over 16 GiB. They include the
runtime, executable pages and startup, and are taken before JSON formatting.
The report also includes create-process peaks, which include sparse fixture
construction. No affinity/cache control or reference-library memory comparison
is performed. These are neither isolated codec RSS nor full-file streaming or
fully allocated disk throughput measurements. OS working set and requested heap
allocations in PERFORMANCE.md measure different things.

Reproduce from the workspace root on Windows/NTFS with the i686 MSVC target:

```sh
python crates/deadlib-audio/decode/wav/tests/run_sparse.py --output target/wav/new-physical-sparse.json
```

The runner refuses existing report/fixture destinations. Large logical fixtures
are generated locally instead of archived as enormous zero-filled WAV files;
the independent marker algorithm, source hashes, header hashes and exact file
geometry make the retained checks reproducible. The report records NTFS with
4096-byte allocation units. Broader filesystems, standard ADPCM block sizes,
sequential throughput and comparative process-memory measurements remain open.
