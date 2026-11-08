# Local Lua 5.1 host-reference capacity patch

Upstream: lua-src 551.0.2 (MIT), copied from the locked Cargo package.

Only `lua-5.1.5/lapi.c` differs. `lua_checkstack` permits up to one million
slots on idle auxiliary threads, which mlua uses to retain Rust handles.
Active C execution keeps the upstream 8,000-slot limit. Native ITGmania
stores actor ownership in the Lua registry instead of a bounded C stack.

The unmodified Mr. Sandman chart builds 192 x 108 actor cells. Its native
capture completes, while the owned host cannot retain even 8,000 Rust handles.
`lua_registry_capacity.rs` checks the actual grid size without importing any
native expectations. The full chart replay remains the acceptance criterion.
