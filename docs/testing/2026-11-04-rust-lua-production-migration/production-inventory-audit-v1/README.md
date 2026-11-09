# Fixed production inventory review

This reviews the original migration inventory. It adds no language, binding or
performance acceptance requirements. The interpreter and C API removal
milestones remain closed.

| Original scope | Reviewed sites |
| --- | ---: |
| Production source files | 84 |
| Source reference lines, including comments and tests | 5,010 |
| Raw C API and identity references | 384 |
| Handle field and parameter candidates | 949 |
| State, limit and GC references | 528 |
| Direct legacy consumers | 3 |
| Normal downstream packages | 10 |

[Reviewed site dispositions](reviewed-site-dispositions.json) retain every
original location, its classification, matching current item guards and review
notes. Frozen source hashes match the recorded baseline. Root module guards,
mixed inherent/legacy trait implementations, noteskin test module inclusion and
the current noteskin oracle's frozen-body audit were checked explicitly.

All 384 raw/identity references belong to isolated legacy items, modules or
pre-existing tests. Remaining handle candidates include `deadlib_lua::Value`,
native JSON/pose values, enum variants and diagnostic text. The original song,
noteskin and GIF production entries dispatch to the shared Rust engine; the
song oracle feature is an explicit development configuration.

This source audit establishes site disposition. Behavioral regressions,
root/reentry/session behavior, the final application graph and linked executable,
rendered smoke and matched measurements are separate requirements of the same
original objective. It does not claim migration completion.

Reproduce with `python audit_fixed_inventory.py`, followed by
`python review_fixed_inventory.py`, from the parent migration evidence directory. The scripts
read current sources and the existing Rust AST scanner output; regenerate the
scanner output if source item positions change.
