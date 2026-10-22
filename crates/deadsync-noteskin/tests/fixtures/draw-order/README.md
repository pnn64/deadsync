These synthetic commands exercise the immediate Actor `draworder` setter before
and after `linear` and `sleep`, including negative values and Lua syntax. They
do not require an installed noteskin.

Native ITGmania's `Actor::SetDrawOrder` writes integer metadata. It does not change
Z, flush a tween, or re-sort the parent's children. `ActorFrame` sorts children
when loading them or when explicitly told to `SortByDrawOrder`; changing the
property during a judgment keeps the existing sibling sequence.

From the repository root, reproduce the native binding and queue assertions:

```text
itgmania-harness-rs actor-conformance crates/deadsync-noteskin/tests/fixtures/draw-order/native-input.json --out target/draw-order-native.json
```

`native-output.json` records the native revision and successful assertions. The
source checks are `Actor.cpp`'s `LunaActor::draworder`, `Actor.h`'s `SetDrawOrder`,
and `ActorFrame.cpp`'s `SortByDrawOrder` and `DrawPrimitives`.
