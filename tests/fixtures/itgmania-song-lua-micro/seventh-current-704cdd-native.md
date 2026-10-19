This is an unedited native draw capture from the current local ITGmania reference,
commit `704cdd5624155f743b59b8b64fedeb0588dae067`, with no tracked source changes.
The private adapter adds observation hooks; it does not change spline or hold math.

Capture with itgmania-harness 0.1.95, after configuring and building the private
adapter against that revision:

```powershell
target/debug/itgmania-harness-rs.exe song-lua-trace 'C:/GitHub/rework/lua-songs/7th Gear/7th Gear.ssc' --itgmania-bin target/native-engine-source/Program/ITGmania-harness-trace.exe --headless --notefield-samples --style single --difficulty Challenge --fallback-theme 'Harness Simply Love' --player-options 'm730, 60% mini, cel' --drawable-from-beat 121.9 --until-beat 130.74 --capture-fps 120 --timeout 180 --max-events 1000000 --out target/seventh-current-704cdd-native.json
```

The JSON is compressed losslessly with zstd level 9. Decompressed SHA-256:
`0547e7882f05e763382f267563e9a0061daa9af62d52692ba788270cbac2c191`.
Its embedded `input.native_source` identifies the compiled engine revision and
hashes of `NoteDisplay.cpp` and `CubicSpline.cpp`. The capture contains 21,228
model draws and 1,420 hold-strip draws, and records `window_visible=false`.

The window includes the reverse hold ending at beat 122.375 and the normal hold
ending at 130.375, plus the first incoming right arrow at 130.75. Match nonzero
render targets to DeadSync's AFT path; target zero is the original Player draw.

This reference itself produces one-local-unit caps in Position spline mode:
`NoteDisplay::DrawHoldPart` fixes the cap beat, zeros ArrowEffects Y, and adds
one normalized tangent unit to rows after the first. It also produces changes
in the first incoming arrow's size and direction near the moving spline bend.
These observations define current-reference parity, not a claim that native
playback is free of those artifacts. No expected vertices were hand-edited.

The compiled-path regression compares consecutive first-arrow poses and motion
against this capture, and tail-cap screen bounds in both scroll directions
within 0.05 design pixels. Curved motion between capture times, discrete point
writes, and endpoint zoom selection also have focused regressions, because
matching isolated rendered poses does not establish temporal parity. Compiled
controls are sampled at 60 Hz; the capture observes native frames at up to 120 Hz,
so the pose comparison retains its explicit 4-pixel and 0.021-scale tolerances.
