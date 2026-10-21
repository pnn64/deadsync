This is an unedited native draw capture from the current local ITGmania reference,
commit `704cdd5624155f743b59b8b64fedeb0588dae067`, with no tracked source changes.
The private adapter adds observation hooks; it does not change spline or hold math.

Capture with itgmania-harness 0.1.95, after configuring and building the private
adapter against that revision:

```powershell
python -B -X utf8 scripts/refresh_native_notefield_fixture.py --harness ../itgmania-harness/target/debug/itgmania-harness-rs.exe --harness-source ../itgmania-harness --native-source ../itgmania-harness/target/native-engine-source --itgmania-bin ../itgmania-harness/target/native-engine-source/Program/ITGmania-harness-trace.exe --output-root target/native-notefield-refresh
```

The JSON was refreshed from the committed song archive with harness 0.1.95 and is compressed losslessly with zstd level 19. Generation receipts and the exact recipe are in `notefield-capture-manifest.json` and `notefield-refresh-inputs.json`. The long compiled-song replay remains explicitly deferred; generation alone is not a passing parity result. Decompressed SHA-256:
`605fa2e2c00b8d716321a7ec2ef400d8508ace2d67490dcb47ca278ccb9373ed`.
Its embedded `input.native_source` identifies the compiled engine revision and
hashes of `NoteDisplay.cpp` and `CubicSpline.cpp`. The capture contains 21,920
model draws and 1,546 hold-strip draws, and records `window_visible=false`.

The window includes the reverse hold ending at beat 122.375 and the normal hold
ending at 130.375, plus the first incoming right arrow at 130.75. Match nonzero
render targets to DeadSync's AFT path; target zero is the original Player draw.

This reference itself produces one-local-unit caps in Position spline mode:
`NoteDisplay::DrawHoldPart` fixes the cap beat, zeros ArrowEffects Y, and adds
one normalized tangent unit to rows after the first. It also produces changes
in the first incoming arrow's size and direction near the moving spline bend.
These observations define current-reference parity, not a claim that native
playback is free of those artifacts. No expected vertices were hand-edited.

The compiled-path regression, when explicitly run, compares consecutive first-arrow poses and motion
against this capture, and tail-cap screen bounds in both scroll directions
within 0.05 design pixels. Curved motion between capture times, discrete point
writes, and endpoint zoom selection also have focused regressions, because
matching isolated rendered poses does not establish temporal parity. Compiled
controls are sampled at 60 Hz; the capture observes native frames at up to 120 Hz,
so the pose comparison retains its explicit 4-pixel and 0.021-scale tolerances.
