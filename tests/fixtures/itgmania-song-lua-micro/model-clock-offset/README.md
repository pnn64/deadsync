# Native Model elapsed clock control

The song has a positive 10 ms simfile offset and the scrolling cyber
noteskin Model used by 321STARS.
Its native song trace retains 61 complete updates over one elapsed second.
The separate actor-conformance request uses a -10 ms BGM origin and feeds
the same elapsed update schedule directly to native ActorFrame and Model.
All 122 UV coordinate checks agree between the two native paths.

Both native JSON documents retain their original bytes; the independent
result is stored with lossless Zstandard compression. `control.json`
uses local relative model paths for repeat captures. The native bridge
restores TimingData's beat-zero origin for SongPosition while texture
updates consume elapsed deltas. Foreground.cpp, Model.cpp and ModelTypes.cpp
establish the distinct clocks; the simfile offset must not shift material
history lookup. The regression compares complete native Model observations
through the production composer. This does not prove framebuffer pixels.

Captured with harness 0.1.51, source e336029, pinned ITGmania 5c737928.
`proof.json` records the CLI and independent native result identities.
