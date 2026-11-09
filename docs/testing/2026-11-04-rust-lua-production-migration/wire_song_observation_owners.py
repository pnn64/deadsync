from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync')/'crates/deadsync-song-lua'
def edit(name,old,new):
 p=r/name;s=p.read_bytes();nl='\r\n' if b'\r\n' in s else '\n';old=old.replace('\n',nl).encode();new=new.replace('\n',nl).encode();assert s.count(old)==1,(name,s.count(old),old[:90]);p.write_bytes(s.replace(old,new))
obs='crate::rust_engine::observations::Observations'
# Root-owned, Rust-only shared capture. No Lua namespace or core changes.
edit('src/rust_engine.rs','pub struct RustSongRuntime {','#[cfg(feature = "test-support")]\nmod observations;\n\npub struct RustSongRuntime {\n    #[cfg(feature = "test-support")]\n    observations: observations::Observations,')
edit('src/rust_engine.rs','        let mut vm = Vm::new();','        #[cfg(feature = "test-support")]\n        let observations = observations::Observations::default();\n        let mut vm = Vm::new();')
edit('src/rust_engine.rs','            crate::song_lua_human_player_count(context),\n        )?;','            crate::song_lua_human_player_count(context),\n            #[cfg(feature = "test-support")]\n            observations.clone(),\n        )?;')
edit('src/rust_engine.rs','let catalog = managers::install(&mut vm, context)?;','let catalog = managers::install(&mut vm, context, #[cfg(feature = "test-support")] observations.clone())?;')
edit('src/rust_engine.rs','let screen = managers::screen::install(&mut vm)?;','let screen = managers::screen::install(&mut vm, #[cfg(feature = "test-support")] observations.clone())?;')
edit('src/rust_engine.rs','            player_status.clone(),\n        )?;','            player_status.clone(),\n            #[cfg(feature = "test-support")]\n            observations.clone(),\n        )?;')
a=(r/'src/rust_engine.rs').read_text();start=a.index('        managers::install_game_state(');end=a.index('        )?;',start)+len('        )?;');block=a[start:end];edit('src/rust_engine.rs',block,block.replace('            context,\n','            context,\n            #[cfg(feature = "test-support")]\n            observations.clone(),\n'))
edit('src/rust_engine.rs','        Ok(Self {\n            vm,','        Ok(Self {\n            #[cfg(feature = "test-support")]\n            observations,\n            vm,')
edit('src/rust_engine/actors.rs','    callback_errors: Vec<String>,','    callback_errors: Vec<String>,\n    #[cfg(feature = "test-support")]\n    observations: super::observations::Observations,')
edit('src/rust_engine/actors.rs','        players: usize,\n    ) -> Result<Self, Error> {','        players: usize,\n        #[cfg(feature = "test-support")] observations: super::observations::Observations,\n    ) -> Result<Self, Error> {')
edit('src/rust_engine/actors.rs','            graph_size: crate::graph_display_body_size(players),','            graph_size: crate::graph_display_body_size(players),\n            #[cfg(feature = "test-support")]\n            observations,')
edit('src/rust_engine/managers.rs','    context: &crate::SongLuaCompileContext,\n) -> Result<deadlib_lua::Root, Error> {',f'    context: &crate::SongLuaCompileContext,\n    #[cfg(feature = "test-support")] observations: {obs},\n) -> Result<deadlib_lua::Root, Error> {{')
edit('src/rust_engine/managers.rs','    catalog::install(vm, context)','    catalog::install(vm, context, #[cfg(feature = "test-support")] observations)')
edit('src/rust_engine/managers/catalog.rs','pub(super) fn install(',f'#[derive(Clone)]\nstruct Setup {{\n    context: SongLuaCompileContext,\n    #[cfg(feature = "test-support")]\n    observations: {obs},\n}}\n\npub(super) fn install(')
edit('src/rust_engine/managers/catalog.rs','    context: &crate::SongLuaCompileContext,\n) -> Result<deadlib_lua::Root, Error> {','    context: &crate::SongLuaCompileContext,\n    #[cfg(feature = "test-support")] observations: '+obs+',\n) -> Result<deadlib_lua::Root, Error> {')
edit('src/rust_engine/managers/catalog.rs','vm.new_host_function_with_data(initialize, &captures, context.clone())?','vm.new_host_function_with_data(initialize, &captures, Setup { context: context.clone(), #[cfg(feature = "test-support")] observations })?')
edit('src/rust_engine/managers/catalog.rs','    let context = call\n        .function_data::<SongLuaCompileContext>()\n        .expect("catalog context")\n        .clone();','    let setup = call.function_data::<Setup>().expect("catalog context").clone();\n    let context = setup.context;')
edit('src/rust_engine/managers/catalog.rs','        super::player_options::install(call, catalog, &context)?;','        super::player_options::install(call, catalog, &context, #[cfg(feature = "test-support")] setup.observations.clone())?;')
# Option method payloads include the shared Rust capture, including lazy methods.
edit('src/rust_engine/managers/player_options.rs','use super::*;','use super::*;\n#[cfg(feature = "test-support")]\nuse crate::rust_engine::observations;')
edit('src/rust_engine/managers/player_options.rs','    context: &crate::SongLuaCompileContext,\n) -> Result<(), Error> {','    context: &crate::SongLuaCompileContext,\n    #[cfg(feature = "test-support")] observations: observations::Observations,\n) -> Result<(), Error> {')
edit('src/rust_engine/managers/player_options.rs','create(call, player, [globals, tonumber, next], setmeta)?','create(call, player, [globals, tonumber, next], setmeta, #[cfg(feature = "test-support")] observations.clone())?')
edit('src/rust_engine/managers/player_options.rs','    setmeta: Value,\n) -> Result<TableId, Error> {','    setmeta: Value,\n    #[cfg(feature = "test-support")] observations: observations::Observations,\n) -> Result<TableId, Error> {')
edit('src/rust_engine/managers/player_options.rs','method_function(call, &captures, method)?','method_function(call, &captures, method, #[cfg(feature = "test-support")] observations.clone())?')
edit('src/rust_engine/managers/player_options.rs','call.new_host_function(index, &captures)?','call.new_host_function_with_data(index, &captures, Index { #[cfg(feature = "test-support")] observations: observations.clone() })?')
edit('src/rust_engine/managers/player_options.rs','fn method_function(','''#[derive(Clone)]
struct Binding {
    method: Method,
    #[cfg(feature = "test-support")]
    observations: observations::Observations,
}
#[derive(Clone)]
struct Index {
    #[cfg(feature = "test-support")]
    observations: observations::Observations,
}
#[cfg(feature = "test-support")]
fn audit(call: &HostCall<'_>) -> observations::Observations {
    call.function_data::<Binding>().unwrap().observations.clone()
}
fn method_function(''')
edit('src/rust_engine/managers/player_options.rs','    method: Method,\n) -> Result<Value, Error> {','    method: Method,\n    #[cfg(feature = "test-support")] observations: observations::Observations,\n) -> Result<Value, Error> {')
edit('src/rust_engine/managers/player_options.rs','    let global = matches!(','    let global = cfg!(feature = "test-support") || matches!(')
edit('src/rust_engine/managers/player_options.rs','call.new_host_function_with_data(binding, &captures, method)','call.new_host_function_with_data(binding, &captures, Binding { method, #[cfg(feature = "test-support")] observations })')
edit('src/rust_engine/managers/player_options.rs','    let method = method_function(call, &captures, Method::Scalar(name.to_ascii_lowercase()))?;','    let data = call.function_data::<Index>().unwrap().clone();\n    let method = method_function(call, &captures, Method::Scalar(name.to_ascii_lowercase()), #[cfg(feature = "test-support")] data.observations)?;')
edit('src/rust_engine/managers/player_options.rs','    let method = call\n        .function_data::<Method>()\n        .expect("player option method")\n        .clone();','    let data = call.function_data::<Binding>().expect("player option method").clone();\n    let method = data.method;')
# PlayerState setter context; its status callbacks retain their original payload.
edit('src/rust_engine/managers/player_options/state.rs','    context: crate::SongLuaCompileContext,','    context: crate::SongLuaCompileContext,\n    #[cfg(feature = "test-support")]\n    observations: observations::Observations,')
edit('src/rust_engine/managers/player_options/state.rs','    status: Arc<Mutex<[PlayerStateStatus; crate::LUA_PLAYERS]>>,\n) -> Result<(), Error> {','    status: Arc<Mutex<[PlayerStateStatus; crate::LUA_PLAYERS]>>,\n    #[cfg(feature = "test-support")] observations: observations::Observations,\n) -> Result<(), Error> {')
edit('src/rust_engine/managers/player_options/state.rs','            context: context.clone(),','            context: context.clone(),\n            #[cfg(feature = "test-support")]\n            observations,')
edit('src/rust_engine/managers/player_options/state.rs','#[derive(Clone)]\nstruct StatusBinding {','#[derive(Clone)]\nstruct Binding {\n    operation: Operation,\n    #[cfg(feature = "test-support")]\n    observations: observations::Observations,\n}\n#[derive(Clone)]\nstruct StatusBinding {')
edit('src/rust_engine/managers/player_options/state.rs','call.new_host_function_with_data(binding, &captures, operation)?','call.new_host_function_with_data(binding, &captures, Binding { operation, #[cfg(feature = "test-support")] observations: setup.observations.clone() })?')
edit('src/rust_engine/managers/player_options/state.rs','    let operation = *call\n        .function_data::<Operation>()\n        .expect("player state operation");','    let data = call.function_data::<Binding>().expect("player state operation").clone();\n    let operation = data.operation;')
edit('src/rust_engine/managers/player_options/state.rs','levels::set_options(call, owners, level, &text)','levels::set_options(call, owners, level, &text, #[cfg(feature = "test-support")] &data.observations)')
# GameState SetCurrentSteps owns the same shared audit, but other bindings do not need it.
edit('src/rust_engine/managers/game_state.rs','    context: crate::SongLuaCompileContext,','    context: crate::SongLuaCompileContext,\n    #[cfg(feature = "test-support")]\n    observations: '+obs+',')
edit('src/rust_engine/managers/game_state.rs','    context: &crate::SongLuaCompileContext,\n) -> Result<(), Error> {','    context: &crate::SongLuaCompileContext,\n    #[cfg(feature = "test-support")] observations: '+obs+',\n) -> Result<(), Error> {')
edit('src/rust_engine/managers/game_state.rs','            context: context.clone(),','            context: context.clone(),\n            #[cfg(feature = "test-support")]\n            observations,')
edit('src/rust_engine/managers/game_state.rs','    let context = call\n        .function_data::<Setup>()\n        .expect("game state setup")\n        .context\n        .clone();','    let setup = call.function_data::<Setup>().expect("game state setup").clone();\n    let context = setup.context;')
p=r/'src/rust_engine/managers/game_state.rs';s=p.read_text();a=s.index('            selection::install(');e=s.index('            )?;',a)+len('            )?;');block=s[a:e];edit('src/rust_engine/managers/game_state.rs',block,block.replace('                call.captures()[7],','                call.captures()[7],\n                #[cfg(feature = "test-support")]\n                setup.observations.clone(),'))
edit('src/rust_engine/managers/game_state/selection.rs','    metadata: Arc<[crate::SongLuaStep]>,','    metadata: Arc<[crate::SongLuaStep]>,\n    #[cfg(feature = "test-support")]\n    observations: '+obs+',')
edit('src/rust_engine/managers/game_state/selection.rs','    messages: Value,\n) -> Result<(), Error> {','    messages: Value,\n    #[cfg(feature = "test-support")] observations: '+obs+',\n) -> Result<(), Error> {')
edit('src/rust_engine/managers/game_state/selection.rs','                metadata: metadata.clone(),','                metadata: metadata.clone(),\n                #[cfg(feature = "test-support")]\n                observations: observations.clone(),')
# Screen callbacks already share Requests, so one Arc clone suffices at publish.
edit('src/rust_engine/managers/screen.rs','    pub next: Option<String>,','    pub next: Option<String>,\n    #[cfg(feature = "test-support")]\n    pub observations: '+obs+',')
edit('src/rust_engine/managers/screen.rs','pub(in crate::rust_engine) fn install(vm: &mut Vm) -> Result<Screen, Error> {','pub(in crate::rust_engine) fn install(vm: &mut Vm, #[cfg(feature = "test-support")] observations: '+obs+') -> Result<Screen, Error> {')
edit('src/rust_engine/managers/screen.rs','Arc::new(Mutex::new(Requests::default()))','Arc::new(Mutex::new(Requests { #[cfg(feature = "test-support")] observations, ..Default::default() }))')
print('Forwarded one test-only shared observation owner through existing bindings')