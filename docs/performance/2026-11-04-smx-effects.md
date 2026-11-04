# SMX effect ticks and result roles, 2026-11-04

Three retained optimizations in `deadsync-smx`, starting at `3c0b8bc19` on
isolated branch `perf/non-lua-kernels-20261104`. Lua and the main worktree's
existing changes remain outside this batch.

## Changes and exactness

1. `PanelFx::tick` resolves background ownership and blackout once per pad,
   then pairs each panel state directly with its output slice. This removes
   repeated option resolution, pad/panel offset arithmetic and indexing.
   Backgrounds still advance before overlays; a blackout still suppresses only
   the background, and an active game overlay still suppresses press feedback.
2. `results_role_candidates` iterates a fixed pair of borrowed suffixes instead
   of allocating and growing a temporary vector. Candidate order, string bytes,
   returned vector capacity and each returned string's capacity stay identical.
   The existing formatting expressions and output allocation strategy remain.
3. `advance_overlay` reads/clamps each frame duration once and resolves loop
   metadata once per call that advances a frame. It retains the original
   subtraction order, threshold comparisons, loop bounds, release behavior and
   roll's terminal hold. Metadata is evaluated after the first threshold check,
   preserving the old lazy evaluation. NaN comparisons still stop advancement.

No public interface, production dependency or state field is added. All changes
use safe Rust. Tests share the existing allocation counter. The production diff
is confined to these three function bodies; the transparency compositor is
unchanged. The adjacent checks file verifies the unchanged surroundings and the
frozen reference against the starting revision.

## Final five-process measurements

The `validated` stage measures the final formatted sources after the background
advance loop was paired directly with its output slots. Cycle columns are
independent medians; throughput is the median of five paired ratios.

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| Idle tick | 203.65 | 160.76 | 1.293x | 1.232-1.331x |
| Background tick | 254.49 | 196.50 | 1.275x | 1.256-1.323x |
| Overlay tick | 426.01 | 364.94 | 1.163x | 1.109-1.218x |
| Background/overlay tick | 1,363.80 | 1,317.22 | 1.040x | 1.029-1.049x |
| Blackout/overlay tick | 428.51 | 372.10 | 1.138x | 1.105-1.189x |
| Quiet overlay advance, 16 calls | 120.83 | 107.77 | 1.121x | 1.105-1.179x |
| Normal overlay advance, 16 calls | 179.00 | 181.75 | 0.983x | 0.982-0.985x |
| Catch-up overlay advance, 16 calls | 628.10 | 461.39 | 1.357x | 1.210-1.557x |
| Base fallback + difficulty roles | 2,265.90 | 1,911.41 | 1.182x | 1.153-1.229x |
| Plain grade + difficulty roles | 932.22 | 811.05 | 1.149x | 1.139-1.198x |
| Base fallback without difficulty | 1,022.43 | 618.33 | 1.587x | 1.524-1.686x |
| Black composition, 64 panels | 4,990.11 | 5,225.85 | 0.955x | 0.911-0.982x |
| Opaque composition, 64 panels | 4,849.19 | 5,109.74 | 0.961x | 0.933-0.966x |
| Alternating composition, 64 panels | 4,804.64 | 5,131.25 | 0.927x | 0.917-0.941x |
| Mixed composition, 64 panels | 4,630.35 | 5,220.37 | 0.922x | 0.886-0.931x |

Ticks build all 1,350 bytes. Quiet/normal/catch-up advances use 0.005 seconds,
1/30 second and 0.2 seconds, respectively. Tick fixtures use 0.005 seconds with
sustained overlays kept alive across rounds. Role timings include creation and
drop of the returned vector and strings.

Normal single-frame advancement has a raw 1.7% cycle cost, about 0.17 cycles per
call in this fixture. Its unchanged-source control is 0.997x with pairs spanning
0.992-1.029x, so this small difference is near the control variation. Catch-up
work and complete overlay ticks improve substantially; no claim of a normal
single-frame speedup is made. Mixed ticks' small gain is also treated cautiously.

The unchanged-source control's fifteen median ratios span 0.928-1.031x;
individual pairs span 0.901-1.063x. The unchanged compositor's four isolated-call
controls are 0.928-0.980x, showing the same directional bias as the final
composition measurements. These isolated call results do not establish a
compositor regression or speedup. Earlier confirmation runs also retain their
raw data: catch-up 1.343x, idle/background ticks 1.393x/1.250x, overlay ticks
1.190x and role creation 1.157-1.561x.

These are CPU component measurements. Worker scheduling, device send rules and
USB traffic are unchanged; LED refresh rate and whole-game performance are not
measured.

## Allocations

All twelve advancement/composition/tick workloads retain zero
allocations, reallocations, frees and byte churn in both variants in every final
process; the raw-data audit asserts each count. Role selection removes one private allocation in every case; grades
with a base fallback also remove its reallocation. Output allocations remain.

| Role fixture | Allocations | Reallocations | Requested bytes | Peak bytes |
| --- | ---: | ---: | ---: | ---: |
| Base + difficulty | 7 -> 6 | 4 -> 3 | 388 -> 308 | 288 -> 256 |
| Plain + difficulty | 5 -> 4 | 0 -> 0 | 138 -> 122 | 122 -> 122 |
| Base, no difficulty | 4 -> 3 | 1 -> 0 | 232 -> 152 | 216 -> 152 |

The counter includes requested bytes across allocations/reallocations. All
returned output is dropped, and freed byte totals match requested byte totals.

## Behavior and build verification

The actual crate passes **112 release unit tests**, with five benchmark/trace
tests left ignored. Four focused tests pass in both debug and release, and the
normal release library check passes. Focused Clippy passes with warnings denied.
The complete crate run has only the workspace's existing unused-patch notices;
the new test fixtures add no warnings.

Four focused tests compare the live implementation with frozen starting code:

- 4,000 deterministic event steps compare all wire bytes and mutable
  playback state, including times as raw float bits. They mix background clocks,
  press/judgement layers, all drive types, release, re-trigger, clear, blackout,
  invalid indices, nonfinite beat positions and nonpositive/NaN tick durations.
- 288 duration/loop/drive scenarios compare 2,304 ticks, including zero,
  negative, NaN and infinite frame durations and out-of-range loop markers.
- 4,096 composition fixtures compare all four layer combinations plus slice
  tails: 20,480 direct output comparisons covering black transparency.
- All 19 grades and seven difficulty inputs compare role order/bytes and
  returned capacities, including absent/empty difficulty, Unicode, separators,
  embedded NUL and a long string.

Every kernel stage emits the same **5,747,871-byte** wire/playback/role trace, SHA-256
`8982ff7c62f85d0693e5d5a354a8a4802c32542bd7678215b28a814814130d2c`.
The trace also records returned role capacities. Debug and release agree.
Immutable animation data is shared between the two engines; trace state covers
each mutable playback field and both complete pad outputs.

## Stages and rejected work

- `baseline`: unchanged starting production, three-process probe.
- `mask-composition`: rejected branchless byte mask, only 0.548-0.636x.
- `pad-resolution`: retained pad-level background resolution, isolated idle/
  background tick medians 1.147x/1.078x.
- `role-suffixes`: retained fixed suffix pair, 1.115-1.547x with fewer temporary
  allocations. The preceding tick change remains enabled.
- `packed-composition`: rejected 24-bit integer selection, only 0.821-0.914x.
- `overlay-metadata`: retained duration/metadata reuse; composition restored.
- `overlay-direct` / `before-overlay`: added quiet/normal/catch-up fixtures,
  then restored the original advancement body as an ablation. Catch-up changes
  from 0.957x in the ablation to 1.401x with metadata reuse in these probes.
- `final` / `confirmation`: five-process checks before the final loop/lint
  cleanup. `validated` is the final five-process source snapshot.
- `control`: five fresh unchanged-source processes, same runtime fixtures.
- `debug` / `debug-validated`: focused unoptimized correctness and trace checks.
- `full-release` / `full-validated`: actual crate tests against shipped assets.

Early stages have twelve workloads and three tests. Direct advancement fixtures
expand this to fifteen workloads; duration-edge coverage brings the final count
to four focused tests. Manifests preserve each stage's actual sources and counts.
The final cleanup removes an indexed background loop from the touched function
and suppresses unused-code/range-loop warnings only on the frozen test module.

## Method and reproduction

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1,
`Intel64 Family 6 Model 79 Stepping 1, GenuineIntel`. The runner extracts the
complete pure effect engine, actual animation structures/constants, result-role
builder and actual Grade enum/suffix methods. The standalone enum omits only
unneeded serialization derives. Frozen production differs only in test access;
state inspection helpers are appended outside those frozen definitions.

Release uses optimization level 3, fat LTO, static MSVC runtime and no debug
information. CPU measurements pin the current thread to CPU 2. Twelve rounds
alternate variants; the first is warmup and the median of eleven remaining
rounds is reported. Allocation counting runs separately from timing. Direct
composition uses identical black-boxed function-pointer dispatch for both
variants. Set-up allocations stay outside measurements.

Adjacent TSVs retain raw thread cycles, elapsed nanoseconds and allocation
counts. Manifests retain source/definition hashes, compiler commands, binaries,
correctness logs and trace hashes. Exact artifacts are archived under
`D:\deadsync-smx-effects-20261104`. Full validation overlays current sources onto
the verified cached dependency workspace, hashes every overlay and extracts
the 34 shipped GIFs from Git. It uses the normal host temporary directory.

```powershell
$env:DEADSYNC_SMX_EFFECTS_ARCHIVE = 'D:\deadsync-smx-effects-20261104'
python scripts/benchmark_smx_effects.py baseline --baseline 3c0b8bc19
python scripts/benchmark_smx_effects.py control --baseline 3c0b8bc19 --runs 5
python scripts/benchmark_smx_effects.py validated --runs 5
python scripts/benchmark_smx_effects.py debug-validated --debug
python scripts/benchmark_smx_effects.py full-validated --full
```
