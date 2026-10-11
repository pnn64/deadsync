This control is captured from the linked ITGmania Model and Actor classes,
using harness 0.1.54 and DeadSync's noteskin asset directory. `native.json`
is the complete, unchanged native trace: 241 updates, 12 Models, no runtime
errors, and no dropped events.

It covers Model's default back-face culling; explicit none/back/front modes;
boolean and numeric `backfacecull`; numeric and legacy enum arguments;
immediate render-state changes during a tween; a queued command; and changes
made by an ActorFrame update callback. The native enum order is back=0,
front=1, none=2. DeadSync's mesh shader encoding is none=0, back=1, front=2.
The comparator maps these explicitly and compares every observation.

The material has no texture. ITGmania omits that snapshot field when texture
sampling is unbound. DeadSync's existing mesh shaders use their built-in
white texel for the same untextured material; the comparator requires that
specific binding and rejects malformed texture values.

Reference behavior is in `Model::Model`, `Actor::SetGlobalRenderStates`,
`LunaActor::backfacecull`, `LunaActor::cullmode`, and `MyLua_checkintboolean`.
See `full_song_lua/native-revalidation-model-culling.json` for source hashes,
native capture provenance, and the unchanged-control before/after results.
