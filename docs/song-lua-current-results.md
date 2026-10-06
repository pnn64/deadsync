# Current song Lua result queue

Resume from this queue. Do not reopen passing results without an observed
failure. Use local resources and the native headless CLI; do not download
resources or launch visible ITGmania instances. Keep the existing semantic
and runtime-modifier comparison scope and tolerances.

The single completion audit at MAIN version 0.5.1795 found 50 passing and
14 failing selected chart results (64 total, including Feelyourtouch's
single and double results). Subsequent targeted verification closes two
failures: **52 verified passes, 12 remaining failures**. This count combines
the audit with those targeted results; the entire audit has not been rerun.

| Closed result | MAIN version | Complete result | Change |
| --- | --- | --- | --- |
| Jumper | 0.5.1796, `190fef272` | 6,411/6,411 | Replace the stale retained capture; keep hash and seed 2 |
| Botanic Panic | 0.5.1797 | 655,940/655,940 | Preserve exact zero-duration setter values; native blink regression |

The audit's other 50 passing results remain closed, including Flying,
Kagetsu, Finite, Bunny House and Lake of Lost Nostalgia. The current audit
scope does not establish whole-game pixel parity or interactive gameplay
parity.

## Remaining observed failures

- LALA
- Mawaru 2
- Mawaru 3
- Mawaru 4
- Mawaru 7
- Mawaru 9
- Seventh Gear
- 666
- Step Your Game Up
- Someone Special
- Waltz Capriccio
- Ultimate Taste

Mawaru 9's observed failure is now a nil `Sprite:GetTexture()` while walking
SongBackground during startup. The earlier Song:GetAllSteps gap is fixed.
Its `_black.png` exists locally. Someone Special's remaining failures are
initial multitap writes; the audit passes its other sections. Mawaru 7 has
two projected-visibility differences. Diagnose those specific failures
before repeating large captures.

## Selection and promotion rules

Keep the frozen project list unchanged. Its 63 entries have 58 exact local
hash matches. Get Into It and Rhythm Hell are unavailable locally. Mawaru
5, Mawaru 8 and Brain Power have different supplied chart hashes; their
available local results pass, and the user will resolve hash mismatches.
Do not download replacements or silently substitute project identities.

For each newly passing result, verify the complete pinned corpus result,
copy only its curated changes into `C:\GitHub\deadsync`, bump the workspace
patch by exactly one in both Cargo.toml and Cargo.lock, and commit there.
Keep the corresponding changes in REWORK uncommitted. Preserve unrelated
work and never copy the entire dirty REWORK tree.

## Local evidence

The original audit and failing observations are in
`C:\GitHub\rework\.tmp\project-completion-audit.log` and
`project-completion-test-status.json`. The immutable audit provenance is
`project-completion-audit.json`. Jumper and Botanic's targeted logs are
`jumper-current-main.log` and `botanic-current-main.log`; their committed
result documents and selected fixtures retain capture provenance.

The curated validation checkout is
`C:\GitHub\rework\.tmp\lua-main-check`. Its Cargo version reflects its
original base; the promoted MAIN version is authoritative. Continue only
the remaining queue and rerun broader tests when a shared production fix
creates a concrete regression concern.
