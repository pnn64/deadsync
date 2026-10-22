# Fof Guys full replay result

## Current production status (2026-10-19)

Production now retains the shared Lua scene and executes the authored physics,
input, timers, sounds and scene transitions. The source-specific Rust FOF
adapter has been removed. The historical compiled reports describe that removed
adapter; use the headless harness's `--production` mode for current gameplay.
The historical replay below covers fewer behaviors and remains separate.

Full-song completion and framebuffer pixel parity remain unverified. See
[the execution audit and historical evidence](song-lua-fof-production-audit.md).

`fof_guys/fof.ssc` **passed** the existing full replay on 2026-10-16:
7,378/7,378 native frames, 129 stage samples, 1,419 stage fields, zero
mismatches, zero callback errors, and exit code 0. Acceptance is the full
replay of implemented comparisons; the acceptance tests were unchanged.

The retained native capture was reused. Its simfile and every archived song
resource match `../rework/lua-songs/fof_guys/`. All comparisons used the
uncompressed candidate, SHA-256
`756b2e53aa3fbe0d7c26f686475280fbca81789d88c58ba1e63b93a9ba6a017d`.
The accepted bytes were then packed with `../itgmania-harness` and their
decompressed SHA-256 verified again.

Both permitted repairs were used and committed promptly:

- `b13ccaa65`: implement native `ActorMultiVertex.LoadTexture`, including
  restoring the default texture on nil without replacing the mesh.
- `cd5f663e8`: apply `zoomtoheight` to empty BitmapText's zoom. Native
  `BitmapText::BuildChars` leaves its constructor height at 1, so the timer's
  `GetZoomY()` must return `1.9199999570846558`, rather than 1.

Both regressions passed on Windows and Linux. Existing mesh, texture, and
text sizing controls passed. Sources, source references, build identity,
and the concrete failures are recorded in the result below.

- [Complete fixture](../tests/fixtures/full_song_lua/50ada90f96b306f583ae02382994d5c4497b33546c02fac6b3f826b0b00913fb.tar.zst)
  (90,340,188 bytes, zstd level 12).
- [Result and repair evidence](../tests/fixtures/full_song_lua/fof-2026-10-16-result.json).
- [Complete replay report](../tests/fixtures/full_song_lua/fof-2026-10-16-replay-report.json.zst).
- [Missing coverage](../tests/fixtures/full_song_lua/fof-2026-10-16-coverage.json).

The reported operation and drawable coverage gaps remain separate from
acceptance. No acceptance gates were added or gaps investigated after the
pass. No other charts were processed. Files over 100 MB remain under the
existing `/target` ignore rule; every committed fixture is below 100 MB.
The earlier 39/39 headless semantic reference remains retained as history.
