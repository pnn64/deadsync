# Mawaru6 parity investigation

The target is the local `mawaru6/mawaru6.sm` dance-single Challenge chart,
description `TaroNuke (converted by MrThatKid4)`, meter 13, frozen hash
`8729524440c7e8d4`. The local chart matches that identity. All references and
song resources used here are local; the frozen project manifest is unchanged.

## Native baseline

The complete capture has two foreground Lua layers, 416,592 events, no runtime
errors and no dropped events. The 78,139,547 raw bytes compress to 2,566,776
bytes with an exact round trip. Raw SHA-256:
`234efa95d8dd3ea5f106210a7656a3000dfdaf806b131d003e52fe4e12b8a782`.
The local ITGmania reference revision is
`5b205125ad53b9867bb4a494ff858f8d38ad4406`.

The initial audit passes 18,191 of 18,666 comparisons, but compiles only the
`_input` foreground layer. A comma inside a commented-out `FGCHANGES` entry
causes the following `0.010=lua` entry to be lost. Missing geometry, messages
and modifiers follow from that missing layer; these are not a whole-song pass.

## Corrections

- Strip MSD line comments before change-tag delimiters and unescaping, matching
  local `MsdFile::ReadBuf`. Preserve escaped slashes and the ending newline.
  The input remains borrowed when there is no actual comment.
- Apply the same preprocessing to foreground/background Lua detection and
  media change parsing, including comments containing tag markers or semicolons.
- Advance song cache version 25 to 26 so existing cached layer lists are
  reparsed. Chart source bytes and frozen hashes remain unchanged.
- A queued callback can replace its clock while starting a recurring cycle.
  Mark the replacement clock after dispatch, preventing a second consumption
  of the same frame delta. This corrects the cats' positions and diffuse values,
  candy and bomb falling positions, and later random colors.
- Resolve extensionless Sprite texture prefixes before dimension getters and
  `zoomtowidth`/`zoomtoheight`. `vidiot/s2` now uses its local 320-by-180 image
  rather than the one-pixel missing-texture size during command compilation.
- Use the captured ScreenGameplay camera as the default for song-layer Sprite
  and Quad rendering. Inner cameras retain their scope; AFT captures retain
  their separate viewport. The semantic audit now reads each sample's camera
  instead of assuming the track's initial orthographic label stays current.

The parser, song Lua and player bridge package suites pass 1,245 tests. A small
native `fg-comment-chain` reference retains both foreground layers across a
comment containing commas, semicolons and a fake tag. MAIN before the parser
fix passes 20 of 22 comparisons and misses the second Quad's geometry.
That guard now passes all 41 comparisons. The additional `queued-cycle-start`
reference improves from 532/622 to 622/622 after the clock correction. The
native `screen-camera-change` fixture covers an extensionless sprite at negative
depth and a zero-size actor behind the camera after a runtime FOV change.
An actual renderer check compares the projected Sprite bounds with that native
capture, including a backward seek.

The retained whole-song audit passes **686,877/686,877**, with zero failures:

| Section | Passed / total |
| --- | ---: |
| Compile info | 8 / 8 |
| Layer order | 3 / 3 |
| Final render | 434 / 434 |
| Render persistence | 118 / 118 |
| Update values | 20,288 / 20,288 |
| Player ranges | 2 / 2 |
| Projected geometry | 204,871 / 204,871 |
| Draw colors | 409,336 / 409,336 |
| Projected vibration | 51,370 / 51,370 |
| Timeline | 130 / 130 |
| Message commands | 277 / 277 |
| Runtime modifiers | 40 / 40 |

Rework passes all 125 regular semantic tests and 1,246 package tests,
1,371 total. The previous complete Mawaru7 regression still passes all
874,665 comparisons. The comment, recurring-cycle and screen-camera guards
pass 41, 622 and 40 comparisons respectively. The actual renderer uses its
reused mesh path and matches the native Sprite bounds within 0.001 pixels.

The complete compressed Mawaru6 reference and provenance are retained under
`tests/fixtures/itgmania-song-lua-micro/mawaru6-whole-song*`, with 193 local
source hashes. This covers no-input headless execution and renderer actors;
it does not claim an interactive GPU screenshot or audible playback check.

## Publication state

MAIN independently passes the same 125 semantic tests and 1,246 package tests,
1,371 total, including the whole Mawaru6 and Mawaru7 comparisons. MAIN is
version `0.5.1753`, exactly one patch above `0.5.1752`; Cargo.toml and Cargo.lock
are included in the curated commit. Frozen chart identities and song sources
are unchanged. Rework retains the changes without committing them.

## Frozen/local identities for user correction

No downloads were used and the frozen manifest remains unchanged. These local
chart identities differ from the frozen project entries:

| Chart | Frozen hash | Local hash |
| --- | --- | --- |
| Mawaru5 | `74765c1936186d20` | `8e0b6274cb33af5e` |
| Mawaru8 | `cadefe09888e9ab8` | `8224fb7e0b05040f` |
| Brain Power - [Cardboard Box] | `a73ec5f2f3015620` | `f40ebaf45ea6e26d` |

The frozen Get Into It (`9c208360a9b25133`) and Rhythm Hell
(`be38aa9e3c88c32b`) chart identities have no match in the available local
inventory. This identifies missing frozen chart matches, not necessarily
missing song folders. Local variant guards do not count as those frozen passes.
