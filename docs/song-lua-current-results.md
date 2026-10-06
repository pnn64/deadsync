# Current project song Lua results

Resume from the listed failures below. Keep accepted passes closed unless
an actual regression appears. Use local resources and the native headless
CLI, with the existing semantic and runtime-modifier scope and tolerances.
Do not download resources or launch visible ITGmania instances.

The frozen project has 63 chart entries. Current evidence:

- **51 passing exact project charts**
- **7 failing exact project charts**
- **3 passing supplied variants with different chart hashes**
- **2 unavailable locally** (Get Into It and Rhythm Hell)

Thus 54 of 61 available listed results pass.
The original single audit also included Mawaru 2–4, which are outside the
frozen list. Their failures are separate and do not expand this work queue.
The audit was run at 0.5.1795; subsequent targeted results supersede only
their own failures. No whole-game pixel or interactive parity claim.

| Closed result | MAIN version | Complete result |
| --- | --- | --- |
| Jumper | 0.5.1796, `190fef272` | 6,411/6,411 |
| Botanic Panic | 0.5.1797, `d4393135b` | 655,940/655,940 |
| LALA | 0.5.1798, `a1116ccb3` | 40,708/40,708 |
| Step Your Game Up | 0.5.1799 | 654,997/654,997 |

## Remaining listed failures

- [Lua] 7th Gear [SX11] (`seventh_gear`)
- [Lua] Ultimate taste [SX15] (`ultimate_taste`)
- [Lua] Waltz Capriccio (`waltz_capriccio`)
- [Lua] 666 (`song_666`)
- [Lua] Someone Special (`someone_special`)
- [Lua] MAWARUCHI SURVIVER (7) (`mawaru7`)
- [Lua] MAWARU SIMULATOR 2016 (9) (`mawaru9`)

Mawaru 9 currently fails on nil `Sprite:GetTexture()` while walking
SongBackground during startup; `_black.png` exists locally. The earlier
Song:GetAllSteps gap is already fixed. Someone Special's failures are
initial multitap writes; other sections pass. Mawaru 7 has two projected
visibility differences. Diagnose these specific failures before large
recaptures.

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
