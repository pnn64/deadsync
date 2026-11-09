use super::*;
use crate::resource_perf_support as support;
use std::hint::black_box;
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

fn installed(data: &noteskin_itg::NoteskinData) -> Session {
    let mut session = Session::new().unwrap();
    session.install("dance", data, "Mirror").unwrap();
    session
}

#[test]
fn ignored_callbacks_preserve_values_identities_argument_effects_and_coroutines() {
    run(
        &mut installed(&super::data()),
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
    );
}

#[cfg(panic = "unwind")]
#[test]
fn ignored_callback_inputs_still_resume_caught_rust_panics() {
    for arguments in ["payload", "{}, payload", "nil, false, {}, payload"] {
        let mut session = installed(&super::data());
        let callback = session
            .vm
            .new_host_function(|_| panic!("ignored noteskin argument panic"), &[])
            .unwrap();
        session.keep(callback).unwrap();
        session.put(session.globals, "panic_arg", callback).unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(
                &mut session,
                &format!("local _, payload=pcall(panic_arg); return cmd({arguments})"),
            );
        }))
        .unwrap_err();
        assert_eq!(
            panic.downcast_ref::<&str>().copied(),
            Some("ignored noteskin argument panic")
        );
        assert_eq!(run(&mut session, "return 6 * 7"), LuaValue::Number(42.));
    }
}

#[test]
fn command_extraction_preserves_holes_coercions_errors_and_callback_effects() {
    super::check_command_extraction(false);
}

#[test]
fn complete_compiled_noteskin_bytes_match_baseline() {
    let directory = support::Directory::new("callback-parity");
    fs::write(directory.0.join("NoteSkin.lua"), LOADER).unwrap();
    let mut data = super::data();
    data.search_dirs = vec![directory.0.clone()];
    let bundle = compile_data("dance", &data, "source", "Mirror").unwrap();
    assert_eq!((bundle.loader.version, bundle.actors.version), (30, 30));
    let bytes = bincode::encode_to_vec(&bundle, bincode::config::standard()).unwrap();
    assert_eq!(
        bytes,
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/callback-version-30.bin"
        ))
        .as_slice()
    );
    assert_eq!(
        bundle.loader.entries.len(),
        4 * crate::compiler::CORE_ELEMENTS.len()
    );
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
pub(super) fn pin() {
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
pub(super) fn measure(name: &str, batch: usize, mut work: impl FnMut()) {
    for sample in 0..10 {
        {
            let start = std::time::Instant::now();
            let first = cycles();
            let (_, counts) = support::measure(|| {
                for _ in 0..batch {
                    work();
                }
            });
            let elapsed_cycles = cycles() - first;
            if sample != 0 {
                println!(
                    "LUA_BOUNDARY_METRIC {}",
                    serde_json::json!({
                        "workload": name, "engine": "rust", "sample": sample,
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
#[ignore = "release benchmark; run alone"]
fn benchmark_noteskin_callbacks_and_commands() {
    pin();
    let directory = support::Directory::new("callback-bench");
    fs::write(directory.0.join("NoteSkin.lua"), LOADER).unwrap();
    let mut data = super::data();
    data.search_dirs = vec![directory.0.clone()];
    let mut session = installed(&data);
    let calls = run(
        &mut session,
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
    );
    run(&mut session, "collectgarbage('stop')");
    measure("noteskin_ignored_callbacks_768", 128, || {
        black_box(session.vm.call(calls, &[]).unwrap());
    });
    for receiver in [false, true] {
        let arguments = if receiver { "state" } else { "" };
        let source = format!(
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
        );
        let runner = run(&mut session, &source);
        let name = if receiver {
            "noteskin_ignored_receivers_768"
        } else {
            "noteskin_ignored_empty_768"
        };
        measure(name, 128, || {
            black_box(session.vm.call(runner, &[]).unwrap());
        });
    }
    for count in [0, 1, 8, 64] {
        let commands = (0..count)
            .map(|index| format!("'x,{index}'"))
            .collect::<Vec<_>>()
            .join(",");
        let actor = run(
            &mut session,
            &format!("return {{__loader_commands={{{commands}}}}}"),
        );
        let expected = (0..count)
            .map(|index| format!("x,{index}"))
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(
            session
                .read_entry("Down", "Tap Note", actor)
                .unwrap()
                .init_command,
            if count == 0 { None } else { Some(expected) }
        );
        measure(&format!("noteskin_commands_{count}"), 1024, || {
            black_box(
                session
                    .scoped(|session| session.read_entry("Down", "Tap Note", actor))
                    .unwrap(),
            );
        });
    }
    measure("noteskin_complete_compile", 4, || {
        black_box(compile_data("dance", &data, "source", "Mirror").unwrap());
    });
}
