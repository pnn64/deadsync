# Current project song Lua results

Resume from the listed failures below. Keep accepted passes closed unless
an actual regression appears. Use local resources and the native headless
CLI, with the existing semantic and runtime-modifier scope and tolerances.
Do not download resources or launch visible ITGmania instances.

The frozen project has 63 chart entries. Current evidence:

- **57 passing exact project charts**
- **1 failing exact project charts**
- **3 passing supplied variants with different chart hashes**
- **2 unavailable locally** (Get Into It and Rhythm Hell)

Thus 60 of 61 available listed results pass.
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
| MAWARUCHI SURVIVER | 0.5.1805 | 1,454,202/1,454,202 |

## Remaining listed failures

- [Lua] MAWARU SIMULATOR 2016 (9) (`mawaru9`)

Mawaru 9 currently fails on nil `Sprite:GetTexture()` while walking the
player Judgment tree during startup: its default Sprite is untextured.
The earlier Song:GetAllSteps gap is already fixed. Diagnose only the
remaining listed failures above.

## Supplied-resource gaps

Keep the frozen list unchanged. The available Mawaru 5, Mawaru 8 and Brain
Power charts have different hashes. Their local results pass; the user
will resolve the hash mismatches. Get Into It and Rhythm Hell have no
local resources. Do not download or silently substitute their identities.

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
