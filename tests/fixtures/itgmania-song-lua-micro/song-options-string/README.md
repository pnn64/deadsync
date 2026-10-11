# Native song options strings

Harness 0.1.54 captured this unmodified control with its compiled
ITGmania `SongOptions::GetString` implementation. `control.lua` applies
each rate at Stage level, which synchronously updates both Song and
Current options, then records `GAMESTATE:GetSongOptionsString()`.

The six observations cover the empty default string, two decimal places,
removal of one trailing zero, a whole non-default rate, and rounding on
either side of a decimal boundary. `native_song_rate_text` compares both
DeadSync string getters to these recorded native messages.

`provenance.json` records the unchanged native source, binary and input
hashes. The complete four-second trace has 241 updates, no runtime errors,
and no dropped events. Different option levels and non-rate song options
remain outside this control's coverage.

Reference: `GameState.cpp` uses
`m_SongOptions.GetCurrent().GetString()` for `GetSongOptionsString`;
`SongOptions.cpp` omits rate 1, formats other rates with `%2.2f`, and
removes one trailing zero. `ModsGroup.h` propagates Stage changes to
Current. These local reference sources were not modified.
