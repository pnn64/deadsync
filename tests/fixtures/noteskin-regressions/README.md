# Noteskin parser regressions

These fixtures replace the `DEADSYNC_3D_SKINS` and `DEADSYNC_ROSEWOOD` test inputs.
The Lua, INI, redirect and MilkShape model files retain the parser inputs from
the corresponding song-local or third-party skin regressions. Each `fixture.json`
lists the source files and their SHA-256 hashes.

The tests copy those committed text files into a temporary directory. They
generate blank PNGs at the recorded dimensions and placeholder audio files for
asset discovery. They do not compare artwork or decode the placeholder audio.
Entries marked `original_placeholder` were already invalid image placeholders;
the fixture gives them a 64-by-64 image instead of relying on a decoding failure.

The Rosewood song fixture keeps the skin inputs and a reduced, authored intro
control. It checks lead-in timing, transition visibility, message beats and sound
asset discovery without loading a full song or detecting DeadSync through a
private Lua global. It retains DeadSync's explicit song-local noteskin support
and verifies that the fixture skin is never installed in the global catalog.

The generated files and parser inputs are test fixtures, not installable artwork
packs. No test reads the original source directories.
