# Replaying compression experiments

The final source in this milestone includes the selected direct-emission matcher.
Each alternative patch changes only `src/zlib.rs`, `src/zlib/deflate.rs` and
`src/zlib/matcher.rs`. Apply one in a separate checkout of this milestone, from
the repository root. Patches are alternatives, not cumulative:

```powershell
git -c core.autocrlf=false apply --check --whitespace=nowarn --directory=crates/deadlib-image/png crates/deadlib-image/png/experiments/lz-tokens16-from-selected.patch
git -c core.autocrlf=false apply --whitespace=nowarn --directory=crates/deadlib-image/png crates/deadlib-image/png/experiments/lz-tokens16-from-selected.patch
```

Do not apply over other codec edits. Raw patch bytes preserve original LF/CRLF
line endings as needed to match the measured source hashes. Every patch was
checked by Git, applied in a fresh temporary checkout, and all three reconstructed
files matched SHA-256 values in [checkpoints.json](checkpoints.json). These hashes
can also be compared with the benchmark manifests. Other production files and
the shared image primitives stayed unchanged from `684326e55`.

| Checkpoint / patch suffix | Measurement artifact prefixes | Change being explored |
| --- | --- | --- |
| `chain16` | `benchmark-lz-chain16-deflate` | Bounded chains, direct fixed output with stored fallback |
| `skip` | `benchmark-lz-skip-deflate`, `benchmark-lz-after-pipeline` | Backoff on sustained literal input |
| `tokens16` | `benchmark-lz-tokens-*`, `benchmark-kodak-selected` | Token planning, sparse insertion, fixed 16-candidate search |
| `search1` | `benchmark-lz-search1-deflate`, `benchmark-kodak-search1` | Fixed one-candidate token search |
| `adaptive` | `benchmark-lz-adaptive-trial-deflate`, `benchmark-kodak-adaptive-trial` | One or 16 candidates, selected by block compression density |
| `adaptive-packed` | `benchmark-lz-packed-trial-deflate`, `benchmark-kodak-packed-trial` | Pack code indices into token records |
| `hash3-packed` | `benchmark-lz-hash3-trial-deflate`, `benchmark-kodak-hash3-trial` | Three-byte instead of four-byte hash |
| `fastpaths` | `benchmark-lz-fastpaths-trial-deflate`, `benchmark-kodak-fastpaths-trial` | Skip the final link lookup and inline hot helpers |
| `direct` | `benchmark-lz-direct-trial-deflate`, `benchmark-kodak-direct-trial` | Direct emission and one deferred history copy per block |
| `codes` | `benchmark-kodak-codes-trial` | Constant-time code indices; same runtime implementation as final |
| `budget3` | `benchmark-kodak-budget3-trial` | Tighten the density threshold; no useful measured gain |

Patch names are `lz-<suffix>-from-selected.patch`. The historical `after` and
`selected` artifact names in the table identify intermediate versions. Final
measurements use `benchmark-lz-final-*` and `benchmark-kodak-final-*`.

The first three synthetic variants have three warmed runs of 20 iterations.
Later exploratory deflate trials have one warmed 20-iteration run; exploratory
corpus trials have one warmed five-iteration run. The token prototype's corpus
has three ten-iteration runs. Final source has three runs of 20 synthetic and ten
corpus iterations. All use logical CPU 0 on a shared host. Exploratory differences
near run-to-run variance did not justify selection; host frequency and other
workloads remain uncontrolled. [BENCHMARKS.md](../BENCHMARKS.md) interprets the
results and records the final implementation's remaining gaps.
