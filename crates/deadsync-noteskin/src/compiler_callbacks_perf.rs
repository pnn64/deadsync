use super::*;
use crate::resource_perf_support as support;
use std::hint::black_box;

#[path = "compiler_callbacks_before.rs"]
mod before;

fn data(root: &Path) -> noteskin_itg::NoteskinData {
    noteskin_itg::NoteskinData {
        name: "callback-fixture".into(),
        overrides: vec![],
        metrics: Default::default(),
        search_dirs: vec![root.to_owned()],
    }
}

const LOADER: &str = r#"
return {Load = function()
    local game = GAMESTATE:GetCurrentGame():GetName()
    local player = GAMESTATE:GetPlayerState(0)
    local options = player:GetPlayerOptionsString('ModsLevel_Preferred')
    assert(game == 'dance' and options == 'Mirror')
    assert(THEME:GetThemeAuthor({}) == '')
    cmd({}, function() end, coroutine.create(function() end))
    return Def.Sprite{Texture = '_blank', InitCommand = function(self)
        self:x(1):y(2):z(3):zoom(0.75):diffusealpha(0.5):visible(true)
    end}
end}
"#;

fn installed(current: bool, data: &noteskin_itg::NoteskinData) -> Lua {
    let lua = Lua::new();
    if current {
        install_host(&lua, "dance", data, "Mirror").unwrap();
    } else {
        before::install_host(&lua, "dance", data, "Mirror").unwrap();
    }
    lua
}

#[test]
fn ignored_callbacks_preserve_values_identities_argument_effects_and_coroutines() {
    let data = data(Path::new(""));
    for current in [false, true] {
        let lua = installed(current, &data);
        lua.load(
            r#"
            local calls = 0
            local function args()
                calls = calls + 1
                return {}, function() end, coroutine.create(function() end), nil, '\255', 0/0
            end
            local state = GAMESTATE:GetPlayerState()
            local game = GAMESTATE:GetCurrentGame()
            for i = 1, 16 do
                assert(GAMESTATE.GetPlayerState(args()) == state)
                assert(GAMESTATE.GetCurrentGame(args()) == game)
                assert(game.GetName(args()) == 'dance')
                assert(state.GetPlayerOptionsString(args()) == 'Mirror')
                assert(THEME.GetThemeAuthor(args()) == '')
                assert(cmd(args()) == nil)
            end
            assert(calls == 96)
            local run = coroutine.create(function()
                assert(game:GetName(args()) == 'dance')
                assert(state:GetPlayerOptionsString(args()) == 'Mirror')
            end)
            assert(coroutine.resume(run))
            assert(calls == 98)
            local loader = loadfile('fixture.lua')
            for i = 1, 8 do
                local actor = loader(args())
                assert(type(actor) == 'table' and actor.__blank == false)
            end
            assert(calls == 106)
            assert(loadfile('fixture.png') == nil)
        "#,
        )
        .exec()
        .unwrap();
    }
}

#[cfg(panic = "unwind")]
#[test]
fn ignored_callback_inputs_still_resume_caught_rust_panics() {
    let data = data(Path::new(""));
    for current in [false, true] {
        for arguments in ["payload", "{}, payload", "nil, false, {}, payload"] {
            let lua = installed(current, &data);
            lua.globals()
                .set(
                    "panic_arg",
                    lua.create_function(|_, ()| -> mlua::Result<()> {
                        panic!("ignored noteskin argument panic")
                    })
                    .unwrap(),
                )
                .unwrap();
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                lua.load(format!(
                    "local _, payload=pcall(panic_arg); return cmd({arguments})"
                ))
                .exec()
            }))
            .unwrap_err();
            assert_eq!(
                panic.downcast_ref::<&str>().copied(),
                Some("ignored noteskin argument panic")
            );
            assert_eq!(lua.load("return 6 * 7").eval::<i32>().unwrap(), 42);
        }
    }
}

#[test]
fn command_extraction_preserves_holes_coercions_errors_and_callback_effects() {
    let lua = Lua::new();
    for source in [
        "return {}",
        "return {'', '  ', 'x,1', '', ' y,2 ', 3, 'z,4'}",
        "return {[1]='x,1',[3]='z,3'}",
        "return {'x,1', false, 'z,3'}",
        "return {'x,1', '\\255'}",
        "return {'x,1', '\\000', ' y,2 '}",
        "return setmetatable({'x,1'}, {__index=function() error('index hook') end})",
    ] {
        let actor = lua.create_table().unwrap();
        actor
            .raw_set(
                "__loader_commands",
                lua.load(source).eval::<Table>().unwrap(),
            )
            .unwrap();
        assert_eq!(
            actor_loader_command(&actor),
            before::actor_loader_command(&actor),
            "{source}"
        );
    }
    let data = data(Path::new(""));
    let lua = installed(true, &data);
    let actor = lua
        .load("return Def.Sprite{InitCommand=function(self) self:x(1):y(2) end}")
        .eval::<Table>()
        .unwrap();
    assert_eq!(
        actor_loader_command(&actor).unwrap().as_deref(),
        Some("x,1;y,2")
    );
    assert_eq!(
        actor_loader_command(&actor).unwrap().as_deref(),
        Some("x,1;y,2;x,1;y,2")
    );
    for command in ["", "  ", "x,1", " x,1 ; y,2 "] {
        let actor = lua.create_table().unwrap();
        actor.raw_set("InitCommand", command).unwrap();
        assert_eq!(
            actor_loader_command(&actor),
            before::actor_loader_command(&actor)
        );
    }
}

#[test]
fn complete_compiled_noteskin_bytes_match_baseline() {
    let root = support::Directory::new("callback-parity");
    fs::write(root.0.join("NoteSkin.lua"), LOADER).unwrap();
    let data = data(&root.0);
    let old = before::compile_data("dance", &data, "source", "Mirror").unwrap();
    let new = compile_data("dance", &data, "source", "Mirror").unwrap();
    let encode = |bundle: &CompiledNoteskinBundle| {
        bincode::encode_to_vec(bundle, bincode::config::standard()).unwrap()
    };
    assert_eq!(encode(&new), encode(&old));
    assert_eq!(new.loader.entries.len(), 4 * CORE_ELEMENTS.len());
}

#[cfg(windows)]
fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut cycles = 0;
    // SAFETY: the pseudo-handle is this live thread; cycles is a valid output.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    cycles
}

#[cfg(windows)]
fn pin() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: the pseudo-handle identifies this worker; CPU 2 matches the song harness.
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
        0
    );
}

#[cfg(windows)]
fn compare(name: &str, batch: usize, mut work: impl FnMut(bool)) {
    for sample in 0..10 {
        for current in [sample % 2 != 0, sample % 2 == 0] {
            let start = std::time::Instant::now();
            let first = cycles();
            let (_, counts) = support::measure(|| {
                for _ in 0..batch {
                    work(black_box(current));
                }
            });
            let elapsed_cycles = cycles() - first;
            if sample != 0 {
                println!(
                    "LUA_BOUNDARY_METRIC {}",
                    serde_json::json!({
                        "workload": name, "current": current, "sample": sample,
                        "operations": batch, "ns": start.elapsed().as_nanos() as u64,
                        "cycles": elapsed_cycles, "allocations": counts.allocs,
                        "reallocations": counts.reallocs, "bytes": counts.allocated_bytes,
                    })
                );
            }
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "paired release benchmark; run alone"]
fn benchmark_noteskin_callbacks_and_commands() {
    pin();
    let root = support::Directory::new("callback-bench");
    fs::write(root.0.join("NoteSkin.lua"), LOADER).unwrap();
    let data = data(&root.0);
    let vms = [installed(false, &data), installed(true, &data)];
    let calls = vms.each_ref().map(|lua| {
        lua.load(
            r#"
        local state, game, arg = GAMESTATE:GetPlayerState(), GAMESTATE:GetCurrentGame(), {}
        return function()
            for i = 1, 128 do
                GAMESTATE:GetPlayerState(arg, game, state)
                GAMESTATE:GetCurrentGame(arg, game, state)
                game:GetName(arg, game, state)
                state:GetPlayerOptionsString(arg, game, state)
                THEME:GetThemeAuthor(arg, game, state)
                cmd(arg, game, state)
            end
        end
    "#,
        )
        .eval::<Function>()
        .unwrap()
    });
    for lua in &vms {
        lua.gc_stop();
    }
    compare("noteskin_ignored_callbacks_768", 128, |current| {
        calls[current as usize].call::<()>(()).unwrap();
    });
    for receiver in [false, true] {
        let arguments = if receiver { "state" } else { "" };
        let runners = vms.each_ref().map(|lua| {
            lua.load(format!(
                r#"
            local state, game = GAMESTATE:GetPlayerState(), GAMESTATE:GetCurrentGame()
            return function()
                for i = 1, 128 do
                    GAMESTATE.GetPlayerState({arguments})
                    GAMESTATE.GetCurrentGame({arguments})
                    game.GetName({arguments})
                    state.GetPlayerOptionsString({arguments})
                    THEME.GetThemeAuthor({arguments})
                    cmd({arguments})
                end
            end
        "#
            ))
            .eval::<Function>()
            .unwrap()
        });
        let name = if receiver {
            "noteskin_ignored_receivers_768"
        } else {
            "noteskin_ignored_empty_768"
        };
        compare(name, 128, |current| {
            runners[current as usize].call::<()>(()).unwrap()
        });
    }
    for count in [0, 1, 8, 64] {
        let actor = vms[0].create_table().unwrap();
        actor
            .raw_set(
                "__loader_commands",
                vms[0]
                    .create_sequence_from((0..count).map(|index| format!("x,{index}")))
                    .unwrap(),
            )
            .unwrap();
        compare(&format!("noteskin_commands_{count}"), 1024, |current| {
            black_box(if current {
                actor_loader_command(&actor)
            } else {
                before::actor_loader_command(&actor)
            })
            .unwrap();
        });
    }
    compare("noteskin_complete_compile", 4, |current| {
        black_box(if current {
            compile_data("dance", &data, "source", "Mirror")
        } else {
            before::compile_data("dance", &data, "source", "Mirror")
        })
        .unwrap();
    });
}
