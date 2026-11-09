exec((__import__('pathlib').Path(__file__).with_name('wire_song_observation_owners.py')).read_text().split('# Root-owned')[0])
edit('src/rust_engine.rs', '''/// An independent Rust VM with the song clock's Rust host bindings installed.
/// All retained Lua objects are explicit roots or traced callback captures.
/// Separate instances have separate heaps and clock state.
#[cfg(feature = "test-support")]
mod observations;

pub struct RustSongRuntime {''', '''#[cfg(feature = "test-support")]
mod observations;

/// An independent Rust VM with the song clock's Rust host bindings installed.
/// All retained Lua objects are explicit roots or traced callback captures.
/// Separate instances have separate heaps and clock state.
pub struct RustSongRuntime {''')
edit('src/rust_engine/managers/player_options.rs', '    let data = call.function_data::<Index>().unwrap().clone();', '    #[cfg(feature = "test-support")]\n    let data = call.function_data::<Index>().unwrap().clone();')
edit('src/rust_engine/managers/player_options/scalars.rs', '''                put(call, state, key, Value::Boolean(value))?;
            }
            return call.push(if chain {''', '''                put(call, state, key, Value::Boolean(value))?;
                #[cfg(feature = "test-support")]
                audit(call).boolean(call, call.captures()[1], owner,
                    observations::Boolean { key, previous, current: value, chained: chain })?;
            }
            return call.push(if chain {''')
# Raw before/after snapshots belong to explicit speed setter attempts, including
# validation failures after partial writes; plain getters never create records.
p=r/'src/rust_engine/managers/player_options/speed.rs'
for name in ['mode','field']:
 old=f"pub(super) fn {name}(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<(), Error> {{"
 edit(str(p.relative_to(r)),old,old+'''
    #[cfg(feature = "test-support")]
    let before = if audit(call).enabled() && (arg(call, 0) != Value::Nil || float(call, arg(call, 1)).is_some()) {
        audit(call).speed_before(call, call.captures()[1], owner)?
    } else { None };''')
s=p.read_text()
start=s.index('pub(super) fn mode(');end=s.index('pub(super) fn other(')
block=s[start:end]
for error in ['speed mod must be finite and greater than zero','negative option speed','Arg must be greater than or equal to zero.']:
 old=f'            return Err(call.error(b"{error}"));'
 assert block.count(old)==1
 block=block.replace(old,'''            #[cfg(feature = "test-support")]
            audit(call).speed(call, call.captures()[1], owner,
                observations::Speed { key, before, failed: true, chained: chained(call) })?;
'''+old)
old='    if call.arguments().len() > 1 && chained(call) {'
assert block.count(old)==2
block=block.replace(old,'''    #[cfg(feature = "test-support")]
    audit(call).speed(call, call.captures()[1], owner,
        observations::Speed { key, before, failed: false, chained: chained(call) })?;
'''+old)
edit(str(p.relative_to(r)),s[start:end],block)
edit('src/rust_engine/managers/player_options.rs', '''            if let Some(value) = text(call, arg(call, 0)) {
                parser::apply(call, owner, &value)?;
            }''', '''            if let Some(value) = text(call, arg(call, 0)) {
                #[cfg(feature = "test-support")]
                let previous = skin_name(call, owner)?;
                parser::apply(call, owner, &value)?;
                #[cfg(feature = "test-support")]
                audit(call).skin(call, call.captures()[1], owner, "fromstring", previous)?;
            }''')
edit('src/rust_engine/managers/player_options.rs', '''            effect(call)?;
        }
    }
    if call.arguments().len() > 1 && chained(call) {''', '''            effect(call)?;
        }
        #[cfg(feature = "test-support")]
        audit(call).skin(call, call.captures()[1], owner, "noteskin", previous.clone())?;
    }
    if call.arguments().len() > 1 && chained(call) {''')
edit('src/rust_engine/managers/player_options/levels.rs','''    text: &str,
) -> Result<(), Error> {''','''    text: &str,
    #[cfg(feature = "test-support")] observations: &observations::Observations,
) -> Result<(), Error> {''')
edit('src/rust_engine/managers/player_options/levels.rs','''    call.with_roots(&[Value::Table(replacement)], |call| {
        if level != 2 {''','''    call.with_roots(&[Value::Table(replacement)], |call| {
        #[cfg(feature = "test-support")]
        let previous = skin_name(call, owners[level])?;
        if level != 2 {''')
edit('src/rust_engine/managers/player_options/levels.rs','''            assign(call, *owner, replacement)?;
        }
        effect(call)''','''            assign(call, *owner, replacement)?;
        }
        #[cfg(feature = "test-support")]
        observations.skin(call, call.captures()[1], owners[level], "setplayeroptions", previous)?;
        effect(call)''')
edit('src/rust_engine/managers/screen.rs', '''    let name = text(call, arg(call, 0));
    let mut stack = call.native_stack()?;''', '''    let name = text(call, arg(call, 0));
    #[cfg(feature = "test-support")]
    if method == "SetNewScreen" && let Some(name) = name.as_ref() {
        let observations = state.lock().unwrap().observations.clone();
        observations.state_call(call, globals, "ScreenManager.SetNewScreen", vec![serde_json::json!(name)])?;
    }
    let mut stack = call.native_stack()?;''')
edit('src/rust_engine/actors/messages.rs', '''                dispatch(call, store, &name, params, true)?;
            }
        }
    }
    call.push(helper(call, MANAGER_HELPER)?)''', '''                dispatch(call, store, &name, params, true)?;
            }
            #[cfg(feature = "test-support")]
            {
                let observations = store.lock().unwrap().observations.clone();
                let globals = helper(call, 5)?;
                observations.state_call(call, globals, "MessageManager.Broadcast", vec![serde_json::json!(String::from_utf8_lossy(&name))])?;
            }
        }
    }
    call.push(helper(call, MANAGER_HELPER)?)''')
edit('src/rust_engine/managers/game_state/selection.rs', '''        write(call, player, Value::Nil)?;
        effect(call)?;''', '''        write(call, player, Value::Nil)?;
        #[cfg(feature = "test-support")]
        data.observations.state_call(call, call.captures()[1], "GameState.SetCurrentSteps", vec![serde_json::json!(player), serde_json::Value::Null])?;
        effect(call)?;''')
edit('src/rust_engine/managers/game_state/selection.rs', '''    effect(call)?;
    call.push(call.captures()[OWNER])''', '''    #[cfg(feature = "test-support")]
    data.observations.state_call(call, call.captures()[1], "GameState.SetCurrentSteps", vec![serde_json::json!(player), serde_json::json!(step.steps_type), serde_json::json!(step.difficulty.sm_name()), serde_json::json!(step.description)])?;
    effect(call)?;
    call.push(call.captures()[OWNER])''')
# Match native startup and chronological capture boundaries. Discovery probes
# after startup are not part of the authored setter history.
edit('src/rust_engine/compiler.rs', '    timer.push_stage("host");', '    timer.push_stage("host");\n    #[cfg(feature = "test-support")]\n    runtime.observations.start();')
edit('src/rust_engine/compiler.rs', '    let (root, mut activation) = loop {', '    let (root, mut activation) = loop {\n        #[cfg(feature = "test-support")]\n        runtime.observations.skin_active(true);')
edit('src/rust_engine/song_activation.rs', '        activation.new_screen = self.screen.state.lock().unwrap().next.take();', '        activation.new_screen = self.screen.state.lock().unwrap().next.take();\n        #[cfg(feature = "test-support")]\n        self.observations.skin_active(false);')
edit('src/rust_engine/compiler.rs', '    let queued = runtime', '    #[cfg(feature = "test-support")]\n    let startup_observations = runtime.observations.take();\n    let queued = runtime')
edit('src/rust_engine/compiler.rs', '    crate::compile::merge_runtime_messages(&mut out.messages, 0, &activation.broadcasts);', '    #[cfg(feature = "test-support")]\n    startup_observations.apply(&mut out);\n    crate::compile::merge_runtime_messages(&mut out.messages, 0, &activation.broadcasts);')
print('Wired setter and state-call observations and startup capture boundaries')
