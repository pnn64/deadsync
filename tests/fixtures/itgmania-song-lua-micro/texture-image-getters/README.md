# Native texture image getters

Ten generated PNG inputs captured with the actual pinned ITGmania
RageBitmapTexture loader. The profile uses 32-bit textures, the native
2048 maximum resolution and enabled high-resolution textures.

Cases separate raw/logical source, prepared image and allocation sizes
for tiny and padded sources, filename stretch, doubleres, resolution
overrides, maximum-size caps and sprite sheets. The Lua regression
checks all six public dimension getters and a retained texture handle
after its owning Sprite loads another file while a second Sprite keeps
the original texture alive. Expected dimensions come
from the native capture, not the DeadSync implementation.

This validates Lua metadata. Default Sprite pixel preparation, physical
allocation/UV mapping and native framebuffer output remain open.

The saved native Lua scene repeats all ten assertions through the actual
RageTexture Lua binding. Its completion markers occur after each
assertion; all ten markers are recorded and no runtime error occurred.
`default.lua`, `getters.sm`, `native-lua.json` and the accompanying
provenance preserve this independent check of the binding and handle
lifetime. The bitmap metadata capture remains the expected-value source.
