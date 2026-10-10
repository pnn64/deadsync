This control checks independent Preferred, Stage, Song and Current options.
Current approaches Song targets with ITGmania's integer microsecond clock;
zero approach speeds freeze it. Fresh Stage and Preferred assignments also
update Current immediately, while direct setters affect only their own level.

`native-control.json` is copied unchanged from the harness's standalone C++
control. It calls compiled ITGmania PlayerOptions, SongOptions and ModsGroup
without the semantic Lua host or DeadSync. The Rust unit test runs the same
assertions through DeadSync's production option APIs and injected clock.
`window-control.json` independently checks that fresh assignments clear
disabled timing windows.

`default.lua` is the unchanged normal Lua API control. `control.lua` is the
same source under the filename required by the unchanged `control.ssc`.
`native.json` contains its complete four-second reference capture: 241 frames,
no Lua errors and no dropped events. The capture uses the semantic Lua loader
with compiled native option methods; it does not use the full C++ song loader.

These checks cover option queries, assignment and approach behavior. Direct
Current writes in gameplay rendering, dynamic music-rate audio clocks, broad
ArrowEffects behavior and framebuffer parity require separate checks. See
`provenance.json` for source hashes, validation results and remaining scope.
