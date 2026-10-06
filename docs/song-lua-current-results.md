# Current project song Lua results

All available listed chart results pass. Keep accepted passes closed unless
an actual regression appears. Use local resources and the native headless
CLI, with the existing semantic and runtime-modifier scope and tolerances.
Do not download resources or launch visible ITGmania instances.

The frozen project has 63 chart entries. Current evidence:

- **58 passing exact project charts**
- **0 failing exact project charts**
- **3 passing supplied variants with different chart hashes**
- **2 unavailable locally** (Get Into It and Rhythm Hell)

Thus 61 of 61 available listed results pass.
The original single audit also included Mawaru 2–4, which are outside the
frozen list. Their failures are separate and do not expand this work queue.
The audit was run at 0.5.1795; subsequent targeted results supersede only
their own failures. No whole-game pixel or interactive parity claim.

| Closed result | MAIN version | Complete result |
| --- | --- | --- |
| Jumper | 0.5.1796, `190fef272` | 6,411/6,411 |
| Botanic Panic | 0.5.1797, `d4393135b` | 655,940/655,940 |
| LALA | 0.5.1798, `a1116ccb3` | 40,708/40,708 |
| Step Your Game Up | 0.5.1799, `da644d501` | 654,997/654,997 |
| 7th Gear | 0.5.1800, `ca9e88ac4` | 1,410,424/1,410,424 |
| 666 | 0.5.1801, `2751f7129` | 222,644/222,644 |
| Waltz Capriccio | 0.5.1802, `12095dbf8` | 148,479/148,479 |
| Ultimate taste | 0.5.1803, `891a189d6` | 309,553/309,553 |
| Ｓｏｍｅｏｎｅ Ｓｐｅｃｉａｌ | 0.5.1804, `2c9f0b3cd` | 1,067,542/1,067,542 |
| MAWARUCHI SURVIVER | 0.5.1805, `c92bf4c47` | 1,454,202/1,454,202 |
| MAWARU SIMULATOR 2016 | 0.5.1806 | 3,663,408/3,663,408 |

## Additional requested results

These charts are outside the frozen 63-entry list. Each passes the full
existing semantic and runtime-modifier audit; no parity fixes were needed.

| Closed result | Complete result | MAIN version |
| --- | --- | --- |
| Spectrum Sequence | 142,520/142,520 | 0.5.1807 |

Capture identities and all exercised section results are recorded in
[song-lua-additional-fixtures.md](song-lua-additional-fixtures.md).

## Remaining listed failures


No available listed result remains failing. Mawaru 9 now receives its
profile judgment texture before startup Lua runs. The earlier
Song:GetAllSteps gap is also fixed. Only the supplied-resource gaps below
remain; do not rerun accepted passes without evidence of a regression.

## Supplied-resource gaps

Keep the frozen list unchanged. The available Mawaru 5, Mawaru 8 and Brain
Power charts have different hashes. Their local results pass; the user
will resolve the hash mismatches. Get Into It and Rhythm Hell have no
local resources. Do not download or silently substitute their identities.

| Chart | Frozen project hash | Supplied local hash |
| --- | --- | --- |
| Mawaru 5 | `74765c1936186d20` | `8e0b6274cb33af5e` |
| Mawaru 8 | `cadefe09888e9ab8` | `8224fb7e0b05040f` |
| Brain Power | `a73ec5f2f3015620` | `f40ebaf45ea6e26d` |

Missing: Get Into It (`9c208360a9b25133`) and Rhythm Hell
(`be38aa9e3c88c32b`). These are content blockers for the frozen 63-entry
list. Preserve the passing local variants and await corrected resources
or a user change to the list; do not repeat their accepted comparisons.

## Promotion and evidence

For each new pass, verify the complete pinned corpus result, copy only its
curated changes into `C:\GitHub\deadsync`, bump the patch by exactly one in
both Cargo.toml and Cargo.lock, and commit there. Keep the corresponding
REWORK changes uncommitted and preserve unrelated work.

Evidence under `C:\GitHub\rework\.tmp`: `project-completion-audit.log`,
`project-completion-test-status.json`, `project-completion-audit.json`,
`current-project-results.json` and each `<song>-current-main.log`.
Per-result documents and selected manifests retain capture provenance.
The curated validation checkout is `lua-main-check`; its base Cargo
version differs from promoted MAIN, whose version is authoritative.
