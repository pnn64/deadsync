//! Existing standalone host unit fixtures, authored on the Rust VM.
use deadlib_lua::{Value, Vm, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {SongRuntime, test_support},
};
use std::path::Path;
use std::sync::Arc;
#[path = "support/baked_program.rs"]
mod api;

fn execute(vm: &mut Vm, source: impl AsRef<[u8]>) -> Vec<Value> {
    let code = compile_named(source.as_ref(), b"=standalone host unit fixture").unwrap();
    vm.execute(&code).unwrap().to_vec()
}
fn runtime(path: &Path) -> SongRuntime {
    SongRuntime::new(&SongLuaCompileContext::new(
        path,
        "standalone host unit fixture",
    ))
    .unwrap()
}

#[test]
fn native_lua51_runtime_uses_native_bytecode_and_coercion() {
    let mut vm = Vm::new();
    let value = execute(&mut vm, b"return string.dump(function() return 1 end)")[0];
    assert_eq!(&api::bytes(&vm, value)[..5], b"\x1bLua\x51");
    execute(
        &mut vm,
        r#"
            assert(tostring(1.0) == '1')
            assert(not pcall(string.format, '%s', true))
            assert(#setmetatable({1}, {__len=function() return 99 end}) == 1)
        "#,
    );
    assert!(compile_named(b"return 1 | 2", b"=invalid Lua 5.1 syntax").is_err());
}

#[test]
fn native_concat_preserves_numbers_and_expressions() {
    let mut vm = Vm::new();
    let source = r#"
            local function many() return 5.0, 99 end
            assert(5.0 .. ' centered' == '5 centered')
            assert('x' .. -5.0 == 'x-5')
            assert(1.25 .. 5.0 == '1.255')
            assert('x' .. 2 + 3 == 'x5')
            assert('x' .. many() == 'x5')
            assert('x' .. (function(...) return ... end)(5.0) == 'x5')
            assert('x' .. -- keep this comment and newline
                5.0 == 'x5')
            assert('..' .. [=[...]=] == '.....')
            local values = {5.0, 'dark'}
            assert(values[1] .. ' ' .. values[2] == '5 dark')
            assert('x' .. ({5.0})[1] == 'x5')
            local object = setmetatable({}, {__concat = function(a,b)
                return type(a) .. ':' .. type(b)
            end})
            assert(5.0 .. object == 'number:table')
            assert(object .. 5.0 == 'table:number')
            assert(not pcall(function() return true .. 'x' end))
            assert(1 .. 2 .. object == '1number:table')
        "#;
    execute(&mut vm, source);
}

#[test]
fn json_to_lua_preserves_arrays_and_objects() {
    let mut host = runtime(Path::new("."));
    let value = serde_json::json!({"name":"song","rows":[1,true,null]});
    let vm = host.vm_mut();
    let decode = vm.get_global(b"JsonDecode").unwrap();
    let input = api::string(vm, serde_json::to_vec(&value).unwrap());
    let table = api::call_one(vm, decode, &[input]);
    assert!(
        matches!(table, Value::Table(_)),
        "object should convert to a table"
    );
    assert_eq!(api::get_bytes(vm, table, b"name"), b"song");
    let rows = api::get(vm, table, b"rows");
    let Value::Table(rows) = rows else {
        panic!("array should convert to a table")
    };
    assert_eq!(
        vm.raw_table_get(rows, Value::Number(1.)).unwrap(),
        Value::Number(1.)
    );
    assert_eq!(
        vm.raw_table_get(rows, Value::Number(2.)).unwrap(),
        Value::Boolean(true)
    );
    assert!(matches!(
        vm.raw_table_get(rows, Value::Number(3.)).unwrap(),
        Value::Nil
    ));
}

#[test]
fn lua_to_json_preserves_dense_array_order() {
    let mut host = runtime(Path::new("."));
    let vm = host.vm_mut();
    let table = api::table(vm);
    let Value::Table(id) = table else {
        unreachable!()
    };
    let a = api::string(vm, b"a");
    let b = api::string(vm, b"b");
    vm.raw_table_set(id, Value::Number(1.), a).unwrap();
    vm.raw_table_set(id, Value::Number(2.), b).unwrap();
    let encode = vm.get_global(b"JsonEncode").unwrap();
    let output = api::call_bytes(vm, encode, &[table]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output).unwrap(),
        serde_json::json!(["a", "b"])
    );
}

#[test]
fn cryptman_returns_fixed_hash_lengths() {
    let mut host = runtime(Path::new("."));
    let vm = host.vm_mut();
    let cryptman = vm.get_global(b"CRYPTMAN").unwrap();
    let sha1 = api::get(vm, cryptman, b"SHA1String");
    let sha1 = api::call_one(vm, sha1, &[]);
    let sha256 = api::get(vm, cryptman, b"SHA256String");
    let sha256 = api::call_one(vm, sha256, &[]);
    assert_eq!(
        matches!(sha1, Value::String(_)) && api::bytes(vm, sha1).len() == 20,
        true
    );
    assert_eq!(
        matches!(sha256, Value::String(_)) && api::bytes(vm, sha256).len() == 32,
        true
    );
}

#[test]
fn rage_file_matches_native_contract() {
    let dir = std::env::temp_dir().join(format!("deadsync-rage-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("file contract directory");
    std::fs::write(dir.join("input.bin"), b"alpha\r\n\nbeta\rZ\0tail").expect("input bytes");
    let mut host = runtime(&dir);
    execute(
        host.vm_mut(),
        r#"
local file = RageFileUtil.CreateRageFile()
assert(not file:Open('missing.txt', 1) and #file:GetError() > 0)
assert(not pcall(function() file:GetLine() end))
assert(file:Flush() == file and file:GetError() == 'Not open')
assert(not file:Open('input.bin', 0))
assert(file:GetError() == 'Neither reading nor writing specified')
assert(not file:Open('input.bin', 3))
assert(file:GetError() == 'Reading and writing are mutually exclusive')
assert(file:Open('input.bin', 1))
assert(file:GetLine() == 'alpha' and file:Tell() == 7 and not file:AtEOF())
assert(file:GetLine() == '' and file:Tell() == 8 and not file:AtEOF())
assert(file:GetLine() == 'beta\rZ' and file:Tell() == 19 and not file:AtEOF())
assert(file:GetLine() == '' and file:AtEOF())
assert(file:Seek(file:Tell()) == 19 and file:AtEOF())
assert(file:Seek(0) == 0 and not file:AtEOF())
assert(file:ReadBytes(7) == 'alpha\r\n' and file:Tell() == 7)
assert(file:ReadBytes(0) == '' and not file:AtEOF())
assert(file:Read() == '\nbeta\rZ' and file:AtEOF())
assert(not pcall(function() file:Write('x') end))
assert(file:Flush() == file)
assert(file:ClearError() == file.ClearError)
assert(file:Close() == file.Close)
assert(file:Open('output.txt', 2))
assert(file:Write('abc') == 3 and file:PutLine('def') == 2)
assert(file:Flush() == file)
assert(not pcall(function() file:Read() end))
file:Close()
local reader = RageFileUtil.CreateRageFile()
assert(reader:Open('output.txt', 1) and reader:Read() == 'abcdef\r\n')
reader:Close()
assert(file:destroy() == file.destroy)
assert(not pcall(function() file:GetLine() end))
"#,
    );
    assert!(
        !dir.join("output.txt").exists(),
        "compile must not write future song state to disk"
    );
    let mut next = runtime(&dir);
    execute(
        next.vm_mut(),
        "assert(not RageFileUtil.CreateRageFile():Open('output.txt', 1))",
    );
}

#[test]
fn mode_tables_match_simply_love_source() {
    let source =
        include_str!("../../../tests/fixtures/itgmania-song-lua-micro/simply-love-mode-tables.lua");
    for rescore in [false, true] {
        let context = SongLuaCompileContext::new(Path::new("."), "Mode Tables");
        let mut host = SongRuntime::new(&context).unwrap();
        execute(
            host.vm_mut(),
            format!("ThemePrefs.Set('RescoreEarlyHits',{rescore})"),
        );
        test_support::rebuild_sl(&mut host, &context).unwrap();
        let expected = execute(host.vm_mut(), source)[0];
        host.vm_mut().set_global(b"expected", expected).unwrap();
        execute(
            host.vm_mut(),
            r#"
                for _, group in ipairs({"Preferences", "Metrics"}) do
                    for mode, reference in pairs(expected[group]) do
                        local actual = SL[group][mode]
                        for key, value in pairs(reference) do
                            assert(actual[key] == value, group .. "." .. mode .. "." .. key)
                        end
                        for key in pairs(actual) do
                            assert(reference[key] ~= nil, "unexpected " .. group .. "." .. mode .. "." .. key)
                        end
                    end
                end
                for mode, reference in pairs(expected.Metrics) do
                    SL.Global.GameMode = mode
                    for key, value in pairs(reference) do
                        local group = (key:find("^Life") or key == "InitialValue")
                            and "LifeMeterBar" or "ScoreKeeperNormal"
                        assert(THEME:HasMetric(group, key), group .. "." .. key)
                        assert(THEME:GetMetric(group, key) == value, mode .. "." .. key)
                        local found = false
                        for _, name in ipairs(THEME:GetMetricNamesInGroup(group)) do
                            if name == key then found = true end
                        end
                        assert(found, "unlisted " .. group .. "." .. key)
                    end
                end
                assert(THEME:GetMetric("Common", "PercentScoreWeightW1") == nil)
            "#,
        );
    }
}

use deadsync_song_lua::{SongLuaDifficulty, SongLuaPlayerContext, SongLuaStep, file_path_string};
use std::{fs, path::PathBuf};
fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-song-lua-crate-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn test_song_steps() -> Vec<SongLuaStep> {
    use SongLuaStep;
    [
        SongLuaDifficulty::Beginner,
        SongLuaDifficulty::Easy,
        SongLuaDifficulty::Medium,
        SongLuaDifficulty::Hard,
        SongLuaDifficulty::Challenge,
        SongLuaDifficulty::Edit,
    ]
    .into_iter()
    .map(|difficulty| SongLuaStep {
        steps_type: "StepsType_Dance_Single".into(),
        difficulty,
        description: String::new(),
        chart_name: String::new(),
        credit: String::new(),
        meter: difficulty.meter().max(0) as u32,
        music_file: String::new(),
        possible_grade_points: 0,
    })
    .collect()
}

#[test]
fn song_steps_use_metadata() {
    let mut context = SongLuaCompileContext::new(".", "real chart order");
    let mut steps = test_song_steps();
    context.song_steps = vec![steps.remove(4), steps.remove(0), steps.remove(3)];
    context.song_steps[2].description = "Mirror".into();
    context.song_steps[2].meter = 17;
    context.player_step_indices = [Some(2), Some(0)];
    let mut host = SongRuntime::new(&context).expect("song metadata and selected players");
    execute(
        host.vm_mut(),
        "song=GAMESTATE:GetCurrentSong();p1=GAMESTATE:GetCurrentSteps(PLAYER_1)",
    );
    execute(
        host.vm_mut(),
        r#"
            local a = song:GetAllSteps()
            assert(#a == 3 and a[1]:GetDifficulty() == 'Difficulty_Challenge')
            assert(a[2]:GetDifficulty() == 'Difficulty_Beginner')
            assert(a[3] == p1 and p1:GetDescription() == 'Mirror' and p1:GetMeter() == 17)
            a[1] = nil
            assert(#song:GetAllSteps() == 3)
            assert(#song:GetStepsByStepsType('StepsType_Dance_Single') == 3)
            assert(#song:GetStepsByStepsType('StepsType_Pump_Single') == 0)
            assert(song:GetOneSteps('StepsType_Dance_Single', 'Difficulty_Hard') == nil)
            assert(not song:HasStepsTypeAndDifficulty('StepsType_Dance_Single', 'Difficulty_Hard'))
            assert(song:HasEdits('StepsType_Dance_Single'))
            assert(song:GetOneSteps('StepsType_Dance_Single', 'Difficulty_Edit') == p1)
        "#,
    );
}

#[test]
fn player_judgment_has_replaceable_sprite_child() {
    let song_dir = test_dir("judgment-sprite-child");
    let texture = song_dir.join("Judgment label 2x6.png");
    image::RgbaImage::new(128, 384).save(&texture).unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "judgment sprite child");
    let mut host = SongRuntime::new(&context).unwrap();
    for player_index in 0..2 {
        context.players[player_index] = SongLuaPlayerContext {
            judgment_texture: (player_index == 0).then(|| texture.clone()),
            ..SongLuaPlayerContext::default()
        };
        let scene = test_support::gameplay_player(&mut host, &context, player_index).unwrap();
        let player = host.actor_scene_root(scene).unwrap();
        let vm = host.vm_mut();
        api::set_string(
            vm,
            player,
            b"__songlua_song_dir",
            song_dir.to_str().unwrap(),
        );
        vm.set_global(b"player", player).unwrap();
        let text = api::string(vm, texture.to_str().unwrap());
        vm.set_global(b"texture", text).unwrap();
        vm.set_global(b"loaded", Value::Boolean(player_index == 0))
            .unwrap();
        let initial = api::string(vm, file_path_string(&texture));
        vm.set_global(b"initial_texture_path", initial).unwrap();
        execute(
            vm,
            r#"
local judgment = player:GetChild("Judgment")
assert(judgment:GetChild("Judgment") == nil)
assert(judgment:GetChild("") == nil)
local sprite = judgment:GetChild("JudgmentWithOffsets")
assert(sprite == judgment:GetChildren().JudgmentWithOffsets)
assert(sprite:GetParent() == judgment)
assert(sprite:GetName() == "JudgmentWithOffsets")
assert(sprite.__songlua_actor_type == "Sprite")
if loaded then
    assert(sprite:GetTexture():GetPath() == initial_texture_path)
    assert(sprite:GetWidth() == 64 and sprite:GetHeight() == 64)
    assert(sprite:GetNumStates() == 12)
else
    assert(sprite:GetTexture() == nil)
end
sprite:Load(texture)
assert(sprite:GetWidth() == 64 and sprite:GetHeight() == 64)
assert(sprite:GetNumStates() == 12)
assert(sprite.Texture == texture)
assert(judgment.Texture == nil)
"#,
        );
    }
}

#[test]
fn lua_session_moves_from_worker_with_its_state() {
    let dir = test_dir("live-lua-worker-transfer");
    fs::write(dir.join("input.bin"), b"abcdef").expect("file cursor input");
    let lua_path = dir.join("default.lua");
    fs::write(
        &lua_path,
        r#"
local steps = GAMESTATE:GetCurrentSteps(PLAYER_1)
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local source_file = RageFileUtil:CreateRageFile()
assert(source_file:Open("input.bin", 1))
assert(source_file:ReadBytes(2) == "ab")
local counter = 0
NEXT_FILE_BYTES = function() return source_file:ReadBytes(2) end
return Def.ActorFrame {
    OnCommand = function(self)
        math.randomseed(1)
        assert(math.random(-10, 10) == -2) -- Independent native random control.
        self:SetUpdateFunction(function(actor, dt)
            assert(GAMESTATE:GetCurrentSteps(PLAYER_1) == steps)
            assert(STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1) == stats)
            counter = counter + 1
            actor:aux(counter)
        end)
    end,
}
"#,
    )
    .expect("live session source");
    let (mut host, scene) = std::thread::spawn(move || {
        let context = SongLuaCompileContext::new(&dir, "Transferred Lua session");
        let mut host = SongRuntime::new(&context).expect("worker-owned Lua host");
        let declaration = host
            .execute_file(&lua_path, &[])
            .expect("worker loads source")[0];
        let scene = host
            .instantiate_actor(declaration)
            .expect("worker initializes scene");
        host.start_actor_scene(scene).expect("worker starts scene");
        // The original startup helper performs this zero-delta initial update.
        host.advance_actor_scene(scene, 0.)
            .expect("startup initial update");
        let snapshot = host.preserve_random();
        execute(host.vm_mut(), "return math.random()");
        drop(snapshot);
        (host, scene)
    })
    .join()
    .expect("worker returns its initialized VM and actors");
    assert_eq!(
        api::as_number(execute(host.vm_mut(), "return math.random()")[0]),
        0.7203244895202072
    );
    let value = execute(host.vm_mut(), "return NEXT_FILE_BYTES()")[0];
    assert_eq!(api::bytes(host.vm_mut(), value), b"cd");
    host.set_values(1.0 / 60.0, 1.0 / 60.0)
        .expect("advance live clock");
    host.advance_actor_scene(scene, 1.0 / 60.0)
        .expect("same callback runs on the receiving thread");
    let root = host
        .actor_scene_root(scene)
        .expect("control returns an ActorFrame");
    let getaux = api::eval(
        host.vm_mut(),
        "return function(actor) return actor:getaux() end",
    )[0];
    assert_eq!(api::call_number(host.vm_mut(), getaux, &[root]), 2.);
    let value = execute(host.vm_mut(), "return NEXT_FILE_BYTES()")[0];
    assert_eq!(api::bytes(host.vm_mut(), value), b"ef");
}

// Both original traversal modes use their production Rust entry points.
fn update_fixture(
    compiled: bool,
    child: bool,
) -> (
    SongRuntime,
    deadsync_song_lua::ActorScene,
    SongLuaCompileContext,
) {
    let mut context = SongLuaCompileContext::new(".", "update traversal fixture");
    context.live = !compiled;
    let mut host = SongRuntime::new(&context).unwrap();
    let screen = if compiled {
        host.begin_song_compilation().unwrap();
        let (screen, selected) = host.construct_gameplay_screen(&context).unwrap();
        host.load_gameplay_theme(screen, &selected).unwrap();
        context = selected;
        host.begin_actor_preparation().unwrap();
        let plan = deadsync_song_lua::SongLayerPlan::new(&[], 0, &context).unwrap();
        host.initialize_song_layers(screen, &plan).unwrap();
        Some(screen)
    } else {
        None
    };
    let declaration = execute(
        host.vm_mut(),
        if child {
            "return Def.ActorFrame{Def.ActorFrame{}}"
        } else {
            "return Def.ActorFrame{}"
        },
    )[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    let root = host.actor_scene_root(scene).unwrap();
    host.vm_mut().set_global(b"root", root).unwrap();
    if child {
        execute(host.vm_mut(), "child=root[1]");
    }
    let scene = if let Some(screen) = screen {
        let parent = host.actor_scene_root(screen).unwrap();
        host.append_actor_scene(scene, parent).unwrap();
        screen
    } else {
        scene
    };
    (host, scene, context)
}
fn prepare_update_fixture(
    host: &mut SongRuntime,
    scene: deadsync_song_lua::ActorScene,
    context: &SongLuaCompileContext,
    compiled: bool,
) {
    if compiled {
        let activation = host.activate_song_layers(scene, context).unwrap().unwrap();
        host.discover_song_startup_commands(scene, &activation)
            .unwrap();
        host.restore_song_startup_locals(scene).unwrap();
        host.begin_song_replay(scene, &[], &activation).unwrap();
    }
}
fn advance_update_fixture(
    host: &mut SongRuntime,
    scene: deadsync_song_lua::ActorScene,
    compiled: bool,
    seconds: &mut f64,
    delta: f64,
) {
    *seconds += delta;
    if compiled {
        host.advance_song_replay(scene, *seconds, delta).unwrap();
    } else {
        host.advance_actor_scene(scene, delta).unwrap();
    }
}
#[test]
fn hibernate_tree_callbacks() {
    for (compiled, owner_rate) in [(false, 2.0), (true, 2.0), (false, 4.0), (true, 4.0)] {
        let (mut host, scene, context) = update_fixture(compiled, true);
        host.vm_mut()
            .set_global(b"owner_rate", Value::Number(owner_rate))
            .unwrap();
        execute(
            host.vm_mut(),
            r#"
events = {}
local function update(label)
    return function(actor, delta)
        assert(actor:GetVisible(), "hibernation preserves GetVisible")
        events[#events + 1] = {label, delta}
    end
end
root:AddWrapperState()
root:GetWrapperState(1):SetUpdateFunction(function(actor, delta)
    update("wrapper")(actor, delta)
    root:SetUpdateRate(owner_rate)
end)
root:SetUpdateRate(2):SetUpdateFunction(update("parent")):hibernate(0.125)
child:SetUpdateRate(3):SetUpdateFunction(update("child"))
"#,
        );
        prepare_update_fixture(&mut host, scene, &context, compiled);
        let mut seconds = 0.;
        for frame in 0..=8 {
            let delta = if frame == 0 {
                0.0
            } else {
                frame as f32 / 60.0 - (frame - 1) as f32 / 60.0
            };
            advance_update_fixture(&mut host, scene, compiled, &mut seconds, f64::from(delta));
            assert_eq!(
                api::as_number(execute(host.vm_mut(), "return #events")[0]),
                if frame == 8 { 3. } else { 0. }
            );
        }
        for (index, label, delta) in [
            (1, "wrapper", 1.0 / 120.0),
            (2, "child", owner_rate / 40.0),
            (3, "parent", owner_rate / 120.0),
        ] {
            let actual = execute(
                host.vm_mut(),
                format!("return events[{index}][1],events[{index}][2]"),
            );
            assert_eq!(api::bytes(host.vm_mut(), actual[0]), label.as_bytes());
            assert!((api::as_number(actual[1]) - delta).abs() < 1.0e-6);
        }
    }
}
#[test]
fn update_rate_binding() {
    let (mut host, scene, _) = update_fixture(false, false);
    let actor = host.actor_scene_root(scene).unwrap();
    host.vm_mut().set_global(b"actor", actor).unwrap();
    execute(
        host.vm_mut(),
        r#"
actor:SetUpdateRate("2")
for _, value in ipairs{0, -1, -math.huge, false, "bad"} do
    assert(not pcall(function() actor:SetUpdateRate(value) end))
    assert(actor:GetUpdateRate() == 2)
end
assert(not pcall(function() actor:SetUpdateRate() end))
assert(pcall(function() actor:SetUpdateRate(0/0) end))
assert(actor:GetUpdateRate() == 2)
actor:SetUpdateRate(math.huge)
assert(actor:GetUpdateRate() == math.huge)
actor:SetUpdateRate(2)
"#,
    );
}
#[test]
fn hibernate_wake_replace() {
    for compiled in [false, true] {
        let (mut host, scene, context) = update_fixture(compiled, false);
        execute(
            host.vm_mut(),
            "calls = 0; root:SetUpdateFunction(function() calls = calls + 1 end):hibernate(math.huge)",
        );
        prepare_update_fixture(&mut host, scene, &context, compiled);
        let mut seconds = 0.;
        for (command, delta, calls) in [
            ("", 10.0, 0),
            ("root:hibernate(0)", 0.0, 1),
            ("root:hibernate(0.25)", 0.125, 1),
            ("root:hibernate(0.03125)", 0.0625, 2),
        ] {
            execute(host.vm_mut(), command);
            advance_update_fixture(&mut host, scene, compiled, &mut seconds, delta);
            assert_eq!(
                api::as_number(execute(host.vm_mut(), "return calls")[0]),
                f64::from(calls)
            );
        }
    }
}

use deadsync_song_lua::{
    SONG_LUA_STARTUP_MESSAGE, SongLuaOverlayEaseBuildParams, SongLuaSpanMode, SongLuaTimeUnit,
    parse_overlay_blend_mode, parse_overlay_effect_clock, parse_overlay_effect_mode,
};
fn set_index(vm: &mut Vm, owner: Value, index: usize, value: Value) {
    let Value::Table(table) = owner else {
        panic!("fixture table expected")
    };
    vm.raw_table_set(table, Value::Number(index as f64), value)
        .unwrap();
}
fn capture_fixture(count: usize) -> (SongRuntime, deadsync_song_lua::ActorScene, Vec<Value>) {
    let context = SongLuaCompileContext::new(".", "source unit capture fixture");
    let mut host = SongRuntime::new(&context).unwrap();
    host.begin_actor_preparation().unwrap();
    let children = std::iter::repeat_n("Def.Quad{}", count)
        .collect::<Vec<_>>()
        .join(",");
    let declaration = api::eval(
        host.vm_mut(),
        format!("return Def.ActorFrame{{{children}}}"),
    )[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    let Value::Table(root) = host.actor_scene_root(scene).unwrap() else {
        unreachable!()
    };
    let actors = (1..=count)
        .map(|i| {
            host.vm_mut()
                .raw_table_get(root, Value::Number(i as f64))
                .unwrap()
        })
        .collect();
    (host, scene, actors)
}
fn capture_actor_global(host: &mut SongRuntime, name: &[u8], actor: Value) {
    host.vm_mut().set_global(name, actor).unwrap();
}
#[test]
fn command_probe_restores_wrappers() {
    let (mut host, _scene, actors) = capture_fixture(2);
    let owner = actors[0];
    capture_actor_global(&mut host, b"owner", owner);
    capture_actor_global(&mut host, b"other", actors[1]);
    execute(
        host.vm_mut(),
        r#"
original = owner:AddWrapperState():x(3)
other_original = other:AddWrapperState():x(7)
owner.ProbeMessageCommand = function(self)
    self:AddWrapperState():x(40)
    other:AddWrapperState():x(50)
    other:GetWrapperState(1):x(90)
end
"#,
    );
    host.capture_song_actor_command(owner, "ProbeMessageCommand")
        .expect("isolated command probe");
    execute(
        host.vm_mut(),
        r#"
assert(owner:GetNumWrapperStates() == 1)
assert(other:GetNumWrapperStates() == 1)
assert(owner:GetWrapperState(1) == original)
assert(other:GetWrapperState(1) == other_original)
assert(original:GetX() == 3)
assert(other_original:GetX() == 7)
"#,
    );
}

#[test]
fn indexed_actor_capture_blocks_preserve_source_indices() {
    let (mut host, _scene, values) = capture_fixture(2);
    capture_actor_global(&mut host, b"captured_actor", values[1]);
    let function = api::eval(host.vm_mut(), "return function() captured_actor:x(12) end")[0];
    let actors = vec![(3, values[0]), (7, values[1])];
    let blocks = host
        .capture_song_actor_function(&actors, function, None, None)
        .unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].0, 7);
    assert_eq!(blocks[0].1[0].delta.x, Some(12.0));
    for actor in &values {
        assert!(
            test_support::captured_blocks(&host, *actor)
                .unwrap()
                .is_empty()
        );
    }
    let empty = api::eval(host.vm_mut(), "return function() end")[0];
    assert!(
        host.capture_song_actor_function(&actors, empty, None, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn capture_indexed_actor_function_blocks_restores_state_and_runtime() {
    let (mut host, _scene, values) = capture_fixture(1);
    let actor = values[0];
    api::set(
        host.vm_mut(),
        actor,
        b"__songlua_visible",
        Value::Boolean(true),
    );
    host.set_values(2., 3.).unwrap();
    capture_actor_global(&mut host, b"captured_actor", actor);
    let function = api::eval(
        host.vm_mut(),
        "local actor=captured_actor; return function(value) local p=GAMESTATE:GetSongPosition();actor:x(value):y(p:GetSongBeat()+p:GetMusicSeconds());actor.__songlua_visible=false end",
    )[0];
    let actors = vec![(5, actor)];
    let blocks = host
        .capture_song_actor_function(&actors, function, Some(12.0), Some(9.0))
        .unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].0, 5);
    assert_eq!(blocks[0].1[0].delta.x, Some(12.0));
    assert_eq!(blocks[0].1[0].delta.y, Some(18.0));
    assert!(api::get_bool(host.vm_mut(), actor, b"__songlua_visible"));
    assert_eq!(host.values().unwrap(), (2.0, 3.0));
}

#[test]
fn capture_overlay_function_eases_builds_probed_overlay_ease() {
    let (mut host, _scene, values) = capture_fixture(2);
    let second = values[1];
    api::set(
        host.vm_mut(),
        second,
        b"__songlua_visible",
        Value::Boolean(true),
    );
    host.set_values(2., 3.).unwrap();
    capture_actor_global(&mut host, b"captured_actor", second);
    let function = api::eval(
        host.vm_mut(),
        "local actor=captured_actor;return function(value)actor:x(value);actor.__songlua_visible=false end",
    )[0];
    let overlays = vec![(0, values[0]), (1, second)];
    let eases = host
        .capture_song_overlay_function_eases(
            &overlays,
            function,
            SongLuaOverlayEaseBuildParams {
                unit: SongLuaTimeUnit::Beat,
                start: 4.,
                limit: 2.,
                span_mode: SongLuaSpanMode::Len,
                easing: None,
                sustain: None,
                opt1: None,
                opt2: None,
            },
            [2., 6.],
            &[second],
        )
        .unwrap();
    assert_eq!(eases.len(), 1);
    assert_eq!(eases[0].overlay_index, 1);
    assert_eq!(eases[0].from.x, Some(2.0));
    assert_eq!(eases[0].to.x, Some(6.0));
    assert!(api::get_bool(host.vm_mut(), second, b"__songlua_visible"));
    assert_eq!(host.values().unwrap(), (2.0, 3.0));
}

#[test]
fn capture_function_action_blocks_collects_captures_and_broadcasts() {
    let (mut host, _scene, values) = capture_fixture(1);
    let actor = values[0];
    api::set(
        host.vm_mut(),
        actor,
        b"__songlua_visible",
        Value::Boolean(true),
    );
    host.set_values(2., 3.).unwrap();
    capture_actor_global(&mut host, b"captured_actor", actor);
    let function = api::eval(
        host.vm_mut(),
        "local actor=captured_actor;return function()actor:x(5);actor.__songlua_visible=false;MESSAGEMAN:Broadcast('Hit') end",
    )[0];
    let overlays = vec![(4, actor)];
    let capture = host
        .capture_song_function_action(&overlays, &[], function, 8.0)
        .unwrap();
    assert_eq!(capture.overlay_blocks.len(), 1);
    assert_eq!(capture.overlay_blocks[0].0, 4);
    assert_eq!(capture.overlay_blocks[0].1[0].delta.x, Some(5.0));
    assert!(capture.tracked_blocks.is_empty());
    assert_eq!(capture.broadcasts, vec![("Hit".to_string(), false)]);
    assert_eq!(capture.side_effects, 1);
    assert!(api::get_bool(host.vm_mut(), actor, b"__songlua_visible"));
    assert_eq!(host.values().unwrap(), (2.0, 3.0));
}

#[test]
fn capture_function_action_blocks_restores_lua_globals() {
    let (mut host, _scene, _) = capture_fixture(0);
    let target = api::table(host.vm_mut());
    api::set(host.vm_mut(), target, b"future_speed", Value::Number(0.));
    let dir = test_dir("function-global-rewind");
    let path = dir.join("default.lua");
    fs::write(
        &path,
        r#"
return function()
    future_speed = 1
    future_only = 2
end
"#,
    )
    .unwrap();
    let loader = host.load_file(&path).unwrap();
    let pin = host.vm_mut().pin(loader).unwrap();
    let setfenv = host.vm_mut().get_global(b"setfenv").unwrap();
    api::call(host.vm_mut(), setfenv, &[loader, target]);
    let function = api::call_one(host.vm_mut(), loader, &[]);
    host.vm_mut().unpin(pin).unwrap();
    host.capture_song_function_action(&[], &[], function, 100.0)
        .unwrap();
    assert_eq!(api::get_number(host.vm_mut(), target, b"future_speed"), 0.);
    assert_eq!(api::get(host.vm_mut(), target, b"future_only"), Value::Nil);
}

#[test]
fn capture_actor_message_commands_reads_blocks_and_skips_failures() {
    let (mut host, _scene, values) = capture_fixture(1);
    let actor = values[0];
    api::set_string(host.vm_mut(), actor, b"Name", "Sample");
    capture_actor_global(&mut host, b"captured_actor", actor);
    execute(
        host.vm_mut(),
        "captured_actor.HitMessageCommand=function(actor)actor:x(9) end;captured_actor.FailMessageCommand=function(actor)error('boom') end",
    );
    let captured = host.capture_song_actor_message_commands(actor).unwrap();
    assert_eq!(captured.commands.len(), 1);
    assert_eq!(captured.commands[0].message, "Hit");
    assert_eq!(captured.commands[0].blocks[0].delta.x, Some(9.0));
    assert_eq!(captured.skipped.len(), 1);
    assert!(captured.skipped[0].contains("FailMessageCommand"));
    assert!(captured.skipped[0].contains("boom"));
}

#[test]
fn capture_actor_message_commands_extracts_startup_sound_blocks() {
    let (mut host, _scene, values) = capture_fixture(1);
    let actor = values[0];
    assert!(test_support::rust_seed_message_sound_block(
        &host, actor, true
    ));
    let captured = host.capture_song_actor_message_commands(actor).unwrap();
    assert_eq!(captured.commands.len(), 1);
    assert_eq!(captured.commands[0].message, SONG_LUA_STARTUP_MESSAGE);
    assert_eq!(captured.commands[0].blocks[0].delta.sound_play, Some(true));
    assert!(captured.skipped.is_empty());
}

#[test]
fn actor_overlay_initial_state_reads_actor_state_fields() {
    let mut host = runtime(Path::new("."));
    let vm = host.vm_mut();
    let actor = api::table(vm);
    let diffuse = api::table(vm);
    for (index, value) in [0.1_f32, 0.2, 0.3, 0.4].into_iter().enumerate() {
        set_index(vm, diffuse, index + 1, Value::Number(f64::from(value)));
    }
    let size = api::table(vm);
    set_index(vm, size, 1, Value::Number(64.));
    set_index(vm, size, 2, Value::Number(32.));
    api::set(vm, actor, b"__songlua_visible", Value::Boolean(false));
    api::set(vm, actor, b"__songlua_state_x", Value::Number(12.));
    api::set(vm, actor, b"__songlua_state_y", Value::Number(-8.));
    api::set(vm, actor, b"__songlua_state_diffuse", diffuse);
    api::set(vm, actor, b"__songlua_state_size", size);
    api::set_string(vm, actor, b"__songlua_state_blend", "BlendMode_Add");
    api::set_string(vm, actor, b"__songlua_state_effect_clock", "beat");
    api::set_string(vm, actor, b"__songlua_state_effect_mode", "glowshift");
    let state = test_support::read_actor_initial_state(&mut host, actor).unwrap();
    assert!(!state.visible);
    assert_eq!(state.x, 12.0);
    assert_eq!(state.y, -8.0);
    assert_eq!(state.diffuse, [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(state.size, Some([64.0, 32.0]));
    assert_eq!(
        state.blend,
        parse_overlay_blend_mode("BlendMode_Add").unwrap()
    );
    assert_eq!(
        state.effect_clock,
        parse_overlay_effect_clock("beat").unwrap()
    );
    assert_eq!(
        state.effect_mode,
        parse_overlay_effect_mode("glowshift").unwrap()
    );
}

#[test]
fn display_child_state_readers_use_named_child_state() {
    let mut host = runtime(Path::new("."));
    let declaration = api::eval(host.vm_mut(), "return Def.GraphDisplay{}")[0];
    let graph_scene = host.instantiate_actor(declaration).unwrap();
    let graph = host.actor_scene_root(graph_scene).unwrap();
    let declaration = api::eval(
        host.vm_mut(),
        "return Def.ActorFrame{Def.Actor{InitCommand=function(self)self:x(5)end},Def.Actor{InitCommand=function(self)self:y(1)end},Def.Actor{InitCommand=function(self)self:y(2)end},Def.Actor{InitCommand=function(self)self:zoom(1.5)end}}",
    )[0];
    let children_scene = host.instantiate_actor(declaration).unwrap();
    let Value::Table(children) = host.actor_scene_root(children_scene).unwrap() else {
        unreachable!()
    };
    let vm = host.vm_mut();
    let line = vm.raw_table_get(children, Value::Number(1.)).unwrap();
    let first_body = vm.raw_table_get(children, Value::Number(2.)).unwrap();
    let second_body = vm.raw_table_get(children, Value::Number(3.)).unwrap();
    let stream = vm.raw_table_get(children, Value::Number(4.)).unwrap();
    let graph_children = api::table(vm);
    api::set(vm, graph_children, b"Line", line);
    let body_group = api::table(vm);
    set_index(vm, body_group, 1, first_body);
    set_index(vm, body_group, 2, second_body);
    api::set(vm, graph_children, b"", body_group);
    api::set(vm, graph, b"__songlua_children", graph_children);
    let graph = host.actor_graph_display(graph).unwrap();
    let line_state = graph.line_state;
    let body_state = graph.body_state;
    assert_eq!(line_state.x, 5.0);
    assert_eq!(body_state.y, 2.0);
    let declaration = api::eval(host.vm_mut(), "return Def.SongMeterDisplay{}")[0];
    let meter_scene = host.instantiate_actor(declaration).unwrap();
    let song_meter = host.actor_scene_root(meter_scene).unwrap();
    let vm = host.vm_mut();
    api::set(
        vm,
        song_meter,
        b"__songlua_stream_width",
        Value::Number(128.),
    );
    let song_meter_children = api::table(vm);
    api::set(vm, song_meter_children, b"Stream", stream);
    api::set(vm, song_meter, b"__songlua_children", song_meter_children);
    let Some(meter) = host.actor_song_meter_display(song_meter).unwrap() else {
        panic!("expected song meter stream state")
    };
    let (stream_width, stream_state) = (meter.stream_width, meter.stream_state);
    assert_eq!(stream_width, 128.0);
    assert_eq!(stream_state.zoom, 1.5);
}

#[test]
fn update_function_table_scans_find_direct_nested_and_global_upvalues() {
    let mut host = runtime(Path::new("."));
    let root = api::eval(
        host.vm_mut(),
        r#"
                local mods = { name = "mods" }
                local nodes = { name = "nodes" }
                local function inner()
                    return nodes
                end
                return {
                    __songlua_update_function = function()
                        return mods
                    end,
                    {
                        __songlua_update_function = function()
                            return inner
                        end
                    }
                }
                "#,
    )[0];
    let mut direct_owner = host
        .read_song_update_function_tables(root, &["mods"])
        .unwrap();
    let mut nested_owner = host
        .read_song_update_function_nested_tables(root, &["nodes"])
        .unwrap();
    let direct = host.song_callback_tables(&direct_owner).unwrap();
    let nested = host.song_callback_tables(&nested_owner).unwrap();
    assert_eq!(direct.len(), 1);
    assert_eq!(api::get_bytes(host.vm_mut(), direct[0], b"name"), b"mods");
    assert_eq!(nested.len(), 1);
    assert_eq!(api::get_bytes(host.vm_mut(), nested[0], b"name"), b"nodes");
    host.release_song_callback_tables(&mut direct_owner)
        .unwrap();
    host.release_song_callback_tables(&mut nested_owner)
        .unwrap();
    let source = api::eval(
        host.vm_mut(),
        r#"
                local nodes = { name = "global" }
                local function inner()
                    return nodes
                end
                return {
                    Run = function()
                        return inner
                    end
                }
                "#,
    )[0];
    host.vm_mut().set_global(b"global_updates", source).unwrap();
    let mut global_owner = host
        .read_song_global_function_nested_tables("global_updates", &["Run"], &["nodes"])
        .unwrap();
    let global = host.song_callback_tables(&global_owner).unwrap();
    assert_eq!(global.len(), 1);
    assert_eq!(api::get_bytes(host.vm_mut(), global[0], b"name"), b"global");
    host.release_song_callback_tables(&mut global_owner)
        .unwrap();
}

#[test]
fn difficulty_from_value_accepts_stepmania_names() {
    let mut context = SongLuaCompileContext::new(".", "difficulty conversion");
    context.song_steps = test_song_steps();
    let mut host = SongRuntime::new(&context).unwrap();
    let selector = api::eval(
        host.vm_mut(),
        "return function(value)local step=GAMESTATE:GetCurrentSong():GetOneSteps('StepsType_Dance_Single',value);if step then return step:GetDifficulty() end end",
    )[0];
    let mut classify = |name: &str| {
        let value = api::string(host.vm_mut(), name);
        let output = api::call_one(host.vm_mut(), selector, &[value]);
        match output {
            Value::Nil => None,
            Value::String(id)
                if host.vm_mut().string_bytes(id).unwrap() == b"Difficulty_Challenge" =>
            {
                Some(SongLuaDifficulty::Challenge)
            }
            other => panic!("unexpected native chart difficulty: {other:?}"),
        }
    };
    assert_eq!(
        classify("Difficulty_Challenge"),
        Some(SongLuaDifficulty::Challenge)
    );
    assert_eq!(classify("expert"), Some(SongLuaDifficulty::Challenge));
    assert_eq!(classify("unknown"), None);
}

#[test]
fn function_named_upvalue_tables_dedups_matching_tables() {
    let mut host = runtime(Path::new("."));
    let function = api::eval(
        host.vm_mut(),
        r#"
                local target = { name = "target" }
                local duplicate = target
                local ignored = { name = "ignored" }
                return function()
                    return target, duplicate, ignored
                end
                "#,
    )[0];
    let probe =
        test_support::function_tables_probe(&mut host, &["target", "duplicate"], false).unwrap();
    let entry = host.vm_mut().rooted(probe).unwrap();
    let out = host.vm_mut().call(entry, &[function]).unwrap().to_vec();
    assert_eq!(out.len(), 1);
    assert_eq!(api::get_bytes(host.vm_mut(), out[0], b"name"), b"target");

    host.vm_mut().unpin(probe).unwrap();
}

#[test]
fn nested_function_named_upvalue_tables_recurses_once_per_function() {
    let mut host = runtime(Path::new("."));
    let function = api::eval(
        host.vm_mut(),
        r#"
                local target = { name = "target" }
                local function inner()
                    return target
                end
                return function()
                    return inner
                end
                "#,
    )[0];
    let probe = test_support::function_tables_probe(&mut host, &["target"], true).unwrap();
    let entry = host.vm_mut().rooted(probe).unwrap();
    let out = host.vm_mut().call(entry, &[function]).unwrap().to_vec();
    assert_eq!(out.len(), 1);
    assert_eq!(api::get_bytes(host.vm_mut(), out[0], b"name"), b"target");
    let second = host.vm_mut().call(entry, &[function]).unwrap().to_vec();
    assert!(second.is_empty());
    host.vm_mut().unpin(probe).unwrap();
}

#[test]
fn theme_pref_default_returns_compat_defaults() {
    let mut host = runtime(Path::new("."));
    let getter = api::eval(
        host.vm_mut(),
        "return function(name)return ThemePrefs.Get(name) end",
    )[0];
    let mut get = |name: &str| {
        let name = api::string(host.vm_mut(), name);
        api::call_one(host.vm_mut(), getter, &[name])
    };
    assert_eq!(get("CasualMaxMeter"), Value::Number(12.));
    assert_eq!(get("SimplyLoveColor"), Value::Number(1.));
    let font = get("ThemeFont");
    let background = get("SongSelectBG");
    assert!(matches!(font,Value::String(id) if host.vm_mut().string_bytes(id).unwrap()==b"Common"));
    assert!(
        matches!(background,Value::String(id) if host.vm_mut().string_bytes(id).unwrap()==b"Off")
    );
    // End the getter's mutable borrow before reading the observed string bytes.
    let name = api::string(host.vm_mut(), "UseImageCache");
    let value = api::call_one(host.vm_mut(), getter, &[name]);
    assert_eq!(value, Value::Boolean(true));
    let name = api::string(host.vm_mut(), "UnknownPreference");
    let value = api::call_one(host.vm_mut(), getter, &[name]);
    assert_eq!(value, Value::Boolean(false));
}

#[test]
fn position_getters_distinguish_live_tween_state_from_destination() {
    let (mut host, scene, actors) = capture_fixture(1);
    let actor = actors[0];
    capture_actor_global(&mut host, b"actor", actor);
    let values = execute(
        host.vm_mut(),
        r#"
actor:x(10)
local before_x, before_dest = actor:GetX(), actor:GetDestX()
actor:decelerate(1):x(5)
return before_x, before_dest, actor:GetX(), actor:GetDestX()
"#,
    );
    let (before_x, before_dest, tween_x, tween_dest) = (
        api::as_number(values[0]),
        api::as_number(values[1]),
        api::as_number(values[2]),
        api::as_number(values[3]),
    );
    assert_eq!((before_x, before_dest), (10.0, 10.0));
    assert_eq!((tween_x, tween_dest), (10.0, 5.0));
    host.set_song_overlay_getter_values(
        scene,
        &[actor],
        &[(
            0,
            deadsync_song_lua::SongLuaOverlayUpdateTarget::X,
            deadsync_song_lua::SongLuaOverlayUpdateValue::F32(7.5),
        )],
    )
    .unwrap();
    let values = execute(host.vm_mut(), "return actor:GetX(), actor:GetDestX()");
    let (live_x, dest_x) = (api::as_number(values[0]), api::as_number(values[1]));
    assert_eq!((live_x, dest_x), (7.5, 5.0));
}

#[test]
fn ensure_overlay_arrow_visual_adds_missing_child_visual() {
    use deadsync_song_lua::{SongLuaOverlayKind, SongLuaOverlayState};
    use std::sync::Arc;
    let context = SongLuaCompileContext::new(".", "arrow visual insertion");
    let mut host = SongRuntime::new(&context).unwrap();
    host.begin_actor_preparation().unwrap();
    let declaration = api::eval(host.vm_mut(), "return Def.ActorFrame{Name='ArrowFrame'}")[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    let root = host.actor_scene_root(scene).unwrap();
    let mut overlays = host
        .read_song_overlay_actors(
            root,
            &context,
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_: &()| None::<deadsync_song_lua::SongLuaOverlayModelLayer<()>>,
            |_| {},
        )
        .unwrap();
    test_support::ensure_multitap_visual(&mut host, &mut overlays, 0, "default", |_| {
        Some((
            SongLuaOverlayKind::Sprite {
                texture_path: PathBuf::from("arrow.png"),
                texture_key: Arc::from("arrow.png"),
                states: Arc::from([]),
                textures: Arc::from([]),
            },
            SongLuaOverlayState::default(),
        ))
    })
    .unwrap();
    assert_eq!(overlays.actors.len(), 2);
    assert_eq!(overlays.actors[1].actor.parent_index, Some(0));
    assert!(matches!(
        overlays.actors[1].actor.kind,
        SongLuaOverlayKind::Sprite { .. }
    ));
    test_support::ensure_multitap_visual(&mut host, &mut overlays, 0, "default", |_| {
        panic!("visual spec should not run when the arrow tree already has a visual")
    })
    .unwrap();
    assert_eq!(overlays.actors.len(), 2);
    host.release_song_overlay_actors(&mut overlays).unwrap();
}

use deadsync_song_lua::{
    SongLuaColumnTransformTarget, SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue,
};
fn get_index(vm: &Vm, owner: Value, index: usize) -> Value {
    let Value::Table(table) = owner else {
        panic!("fixture table expected")
    };
    vm.raw_table_get(table, Value::Number(index as f64))
        .unwrap()
}
fn column_fixture() -> (SongRuntime, Value) {
    let mut host = runtime(Path::new("."));
    let scene = test_support::note_field(&mut host, 0, "dance-single").unwrap();
    let field = host.actor_scene_root(scene).unwrap();
    host.vm_mut().set_global(b"field", field).unwrap();
    execute(
        host.vm_mut(),
        "columns=field:GetColumnActors();column=columns[1]",
    );
    let vm = host.vm_mut();
    let parent = api::table(vm);
    let children = api::table(vm);
    api::set(vm, children, b"NoteField", field);
    api::set(vm, parent, b"__songlua_children", children);
    vm.set_global(b"__songlua_top_screen_player_1", parent)
        .unwrap();
    (host, field)
}
#[test]
fn update_capture_preserves_shared_actor_indices() {
    let directory = tempfile::tempdir().unwrap();
    let entry = directory.path().join("default.lua");
    std::fs::write(&entry, "return Def.ActorFrame{InitCommand=function(self)root=self end,Def.ActorFrame{InitCommand=function(self)child=self end}}")
        .unwrap();
    let context = SongLuaCompileContext::new(directory.path(), "shared capture indices");
    let mut host = SongRuntime::new(&context).unwrap();
    host.begin_song_compilation().unwrap();
    let (screen, context) = host.construct_gameplay_screen(&context).unwrap();
    host.load_gameplay_theme(screen, &context).unwrap();
    let plan = deadsync_song_lua::SongLayerPlan::new(&[&entry], 0, &context).unwrap();
    host.initialize_song_layers(screen, &plan).unwrap().unwrap();
    let shared = host.vm_mut().get_global(b"root").unwrap();
    let other = host.vm_mut().get_global(b"child").unwrap();
    let Value::Table(shared_id) = shared else {
        unreachable!()
    };
    let Value::Table(other_id) = other else {
        unreachable!()
    };
    let activation = host
        .activate_song_layers(screen, &context)
        .unwrap()
        .unwrap();
    host.discover_song_startup_commands(screen, &activation)
        .unwrap();
    host.restore_song_startup_locals(screen).unwrap();
    host.begin_song_replay(
        screen,
        &[
            (shared_id, deadsync_song_lua::SongLuaOverlayState::default()),
            (other_id, deadsync_song_lua::SongLuaOverlayState::default()),
            (shared_id, deadsync_song_lua::SongLuaOverlayState::default()),
        ],
        &activation,
    )
    .unwrap();
    assert!(host.set_song_replay_frames(screen, &[(0.0, 0.0)], None));
    execute(host.vm_mut(), "root:x(23);child:y(45)");
    let rows = host.drain_song_overlay_updates(screen).unwrap();
    let shared_mask = rows
        .iter()
        .find(|row| row.overlay_index == 2)
        .unwrap()
        .values
        .iter()
        .fold(0u128, |mask, (target, _)| {
            mask | (1u128 << *target as usize)
        });
    assert_eq!(shared_mask, 1 << SongLuaOverlayUpdateTarget::X as usize);
    let writes = rows
        .into_iter()
        .map(|row| (row.overlay_index, row.values))
        .collect::<Vec<_>>();
    assert_eq!(
        writes,
        [
            (
                2,
                vec![(
                    SongLuaOverlayUpdateTarget::X,
                    SongLuaOverlayUpdateValue::F32(23.0)
                )]
            ),
            (
                1,
                vec![(
                    SongLuaOverlayUpdateTarget::Y,
                    SongLuaOverlayUpdateValue::F32(45.0)
                )]
            ),
        ]
    );
}

#[test]
fn spline_storage_matches_native_layout() {
    let (mut host, field) = column_fixture();
    execute(
        host.vm_mut(),
        r#"
            local handler = column:GetZoomHandler()
            handler:SetSplineMode("NoteColumnSplineMode_Offset"):SetSubtractSongBeat(false)
            spline = handler:GetSpline()
            assert(spline:GetSize() == 0)
            spline:SetSize(8.9)
            assert(spline:get_size() == 8)
            local point = {-1,-1,-1}
            spline:SetPoint(8.9, point):Solve()
            point[1] = 12
            assert(not pcall(function() spline:SetPoint(9, {0,0,0}) end))
            assert(not pcall(function() spline:SetPoint(0, {0,0,0}) end))
            assert(not pcall(function() spline:SetPoint(1, false) end))
            assert(not pcall(function() spline:SetSize(-1) end))
        "#,
    );
    let windows = host.read_song_note_column_zoom_hides().unwrap();
    assert_eq!(
        windows.len(),
        1,
        "copied last knot stays hidden after table mutation"
    );
    assert_eq!(windows[0].spline_size, 8);
    assert_eq!((windows[0].start_beat, windows[0].end_beat), (7.0, 7.0));
    execute(host.vm_mut(), "spline:set_size(3):set_size(8):solve()");
    assert!(
        host.read_song_note_column_zoom_hides().unwrap().is_empty(),
        "regrown knots are initialized to zero"
    );
    let samples = host
        .read_song_note_column_transform_samples_for_fields(&[field])
        .unwrap();
    let zoom = samples
        .iter()
        .find(|sample| sample.column == 0 && sample.target == SongLuaColumnTransformTarget::Zoom)
        .expect("implicit zero zoom knots");
    assert_eq!(zoom.value, 1.0);
    execute(
        host.vm_mut(),
        r#"
            column:GetPosHandler():SetSplineMode("NoteColumnSplineMode_Position")
                :GetSpline():SetSize(8):SetPoint(8, {"9"}):Solve()
        "#,
    );
    let mut capture = deadsync_song_lua::SongColumnSplineCapture::default();
    host.capture_song_column_splines(&mut capture, 0.).unwrap();
    execute(
        host.vm_mut(),
        r#"
            spline = column:GetPosHandler():GetSpline()
            spline:set_size(4):set_loop(true)
            for i, x in ipairs({0, 96, 0, -96}) do spline:set_point(i, {x,0,0}) end
            spline:solve()
            assert(spline:get_loop())
            assert(spline:evaluate(-0.25)[1] == spline:evaluate(3.75)[1])
            assert(spline:evaluate(4.5)[1] == spline:evaluate(0.5)[1])
            assert(spline:evaluate(0.5)[1] == 66)
        "#,
    );
    host.capture_song_column_splines(&mut capture, 1.).unwrap();
    execute(host.vm_mut(), "spline:set_loop(false):solve()");
    host.capture_song_column_splines(&mut capture, 2.).unwrap();
    let tracks = capture.finish().unwrap();
    let track = tracks
        .iter()
        .find(|track| track.player == 0 && track.column == 0)
        .unwrap();
    let frames = &track.frames;
    let position = frames[0].position.as_ref().expect("position spline");
    assert_eq!(position.coefficients.len(), 8);
    for knot in &position.coefficients[..7] {
        assert_eq!([knot[0][0], knot[1][0], knot[2][0]], [0.0; 3]);
    }
    let last = position.coefficients[7];
    assert_eq!([last[0][0], last[1][0], last[2][0]], [9.0, 0.0, 0.0]);
    let looped = frames[1].position.as_ref().unwrap();
    assert!(looped.looping);
    let straight = frames[2].position.as_ref().unwrap();
    assert!(!straight.looping);
    assert_ne!(straight.coefficients, looped.coefficients);
}

#[test]
fn spline_witness_edits() {
    let (mut host, field) = column_fixture();
    execute(
        host.vm_mut(),
        r#"
            local handler = columns[1]:GetZoomHandler()
            handler:SetSplineMode("NoteColumnSplineMode_Offset")
            spline = handler:GetSpline()
            spline:SetSize(8)
            for i=1,8 do spline:SetPoint(i, {0, 0, 0}) end
            spline:SetPoint(8, {-1, 0, 0})
        "#,
    );
    let zoom = |host: &mut SongRuntime| {
        host.read_song_note_column_transform_samples_for_fields(&[field])
            .unwrap()
            .into_iter()
            .find(|sample| {
                sample.column == 0 && sample.target == SongLuaColumnTransformTarget::Zoom
            })
            .map(|sample| sample.value)
    };
    assert_eq!(zoom(&mut host), None);
    assert_eq!(zoom(&mut host), None);
    let spline = host.vm_mut().get_global(b"spline").unwrap();
    let points = api::get(host.vm_mut(), spline, b"__songlua_spline_points");
    let point = get_index(host.vm_mut(), points, 8);
    set_index(host.vm_mut(), point, 1, Value::Number(0.));
    assert_eq!(zoom(&mut host), Some(1.0));
    let point = get_index(host.vm_mut(), points, 4);
    set_index(host.vm_mut(), point, 1, Value::Number(-0.5));
    assert_eq!(zoom(&mut host), None);
    assert_eq!(zoom(&mut host), None);
    execute(host.vm_mut(), "spline:SetSize(3)");
    assert_eq!(zoom(&mut host), Some(1.0));
    let first = get_index(host.vm_mut(), points, 1);
    set_index(host.vm_mut(), first, 1, Value::Number(0.5));
    assert_eq!(zoom(&mut host), None);
    for i in 2..=3 {
        let point = get_index(host.vm_mut(), points, i);
        set_index(host.vm_mut(), point, 1, Value::Number(0.5));
    }
    assert_eq!(zoom(&mut host), Some(1.5));
}

fn unit_clock_set(
    host: &mut SongRuntime,
    owner: Value,
    key: &str,
    value: f64,
) -> Result<(), deadlib_lua::Error> {
    let vm = host.vm_mut();
    let Value::Table(table) = owner else {
        panic!("native runtime table")
    };
    let key = vm.try_intern(key.as_bytes())?;
    vm.raw_table_set(table, key, Value::Number(value))
}
#[test]
fn music_seconds_preserves_offset_and_rate() {
    for (origin, rate) in [
        (0.0, 1.0),
        (1.25, 1.0),
        (-0.5, 1.0),
        (1.25, 2.0),
        (-0.5, 0.5),
    ] {
        let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "music clock");
        context.song_music_rate = rate;
        context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
            origin,
            0.0,
            &deadsync_rules::timing::TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..Default::default()
            },
            &[],
        ));
        let mut host = SongRuntime::new(&context).unwrap();
        let runtime = host.runtime_table().expect("song runtime");
        let position = host.song_position().expect("song position");
        assert_eq!(
            context
                .song_timing
                .as_ref()
                .expect("native song timing")
                .get_time_for_beat_exact(0.0),
            origin
        );
        for elapsed in [0.0, 1e-9, 1.0 / 60.0, 0.5, 2.0] {
            unit_clock_set(
                &mut host,
                runtime,
                deadsync_song_lua::SONG_LUA_RUNTIME_SECONDS_KEY,
                elapsed,
            )
            .expect("trace timestamp");
            for method in ["GetMusicSeconds", "GetMusicSecondsVisible"] {
                let getter = Ok::<_, deadlib_lua::Error>(api::get(
                    host.vm_mut(),
                    position,
                    method.as_bytes(),
                ))
                .expect("native getter");
                let actual =
                    Ok::<_, deadlib_lua::Error>(
                        api::call_number(host.vm_mut(), getter, &[position]) as f32,
                    )
                    .expect("music timestamp");
                assert_eq!(
                    actual,
                    (elapsed * f64::from(rate)) as f32 + origin,
                    "{method}: origin={origin}, rate={rate}, elapsed={elapsed}"
                );
            }
            assert_eq!(
                Ok::<_, deadlib_lua::Error>(api::get_number(
                    host.vm_mut(),
                    runtime,
                    deadsync_song_lua::SONG_LUA_RUNTIME_SECONDS_KEY.as_bytes()
                ))
                .expect("wall clock"),
                elapsed
            );
        }
    }
}

#[test]
fn arrow_offset_uses_native_music_clock() {
    use deadsync_rules::timing::{
        DelaySegment, SpeedSegment, SpeedUnit, StopSegment, TimingData, TimingSegments,
    };
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-actors/pause-offset.json"
    ))
    .expect("native pause offset control");
    let mut failures = Vec::new();
    for map in native["maps"].as_array().expect("native timing maps") {
        let name = map["name"].as_str().expect("timing map name");
        let timing = TimingData::from_segments(
            0.125,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                stops: if name == "stop" {
                    vec![StopSegment {
                        beat: 4.0,
                        duration: 1.0,
                    }]
                } else {
                    vec![]
                },
                delays: if name == "delay" {
                    vec![DelaySegment {
                        beat: 4.0,
                        duration: 1.0,
                    }]
                } else {
                    vec![]
                },
                speeds: vec![
                    SpeedSegment {
                        beat: 0.0,
                        ratio: 0.5,
                        delay: 0.0,
                        unit: SpeedUnit::Beats,
                    },
                    SpeedSegment {
                        beat: 4.0,
                        ratio: 2.0,
                        delay: 2.0,
                        unit: SpeedUnit::Seconds,
                    },
                ],
                ..Default::default()
            },
            &[],
        );
        for rate in [0.5, 0.75, 1.0, 1.25, 1.5, 2.0] {
            let mut context = SongLuaCompileContext::new(".", "native pause offsets");
            context.song_timing = Some(timing.clone());
            context.player_timing[0] = Some(timing.clone());
            context.player_timing[1] = Some(timing.clone());
            context.song_music_rate = rate;
            let mut host = SongRuntime::new(&context).expect("native song metadata");
            let runtime = host.runtime_table().expect("song runtime");
            let players = api::eval(
                host.vm_mut(),
                "return GAMESTATE:GetPlayerState(PLAYER_1),GAMESTATE:GetPlayerState(PLAYER_2)",
            );
            let effects = host
                .vm_mut()
                .get_global(b"ArrowEffects")
                .expect("ArrowEffects");
            let offset = api::get(host.vm_mut(), effects, b"GetYOffset");
            for sample in map["samples"].as_array().expect("native samples") {
                let elapsed = sample["elapsed"].as_f64().expect("elapsed timestamp") as f32;
                let beat = sample["beat"].as_f64().expect("native beat") as f32;
                let note = sample["note"].as_f64().expect("note beat") as f32;
                let expected = sample["offset"].as_f64().expect("native offset") as f32;
                unit_clock_set(
                    &mut host,
                    runtime,
                    deadsync_song_lua::SONG_LUA_RUNTIME_BEAT_KEY,
                    f64::from(beat),
                )
                .expect("native update beat");
                unit_clock_set(
                    &mut host,
                    runtime,
                    deadsync_song_lua::SONG_LUA_RUNTIME_SECONDS_KEY,
                    f64::from(elapsed) / f64::from(rate),
                )
                .expect("replay wall clock");
                for state in [Value::Nil, players[0], players[1]] {
                    let actual = Ok::<_, deadlib_lua::Error>(api::call_number(
                        host.vm_mut(),
                        offset,
                        &[state, Value::Number(1.0), Value::Number(f64::from(note))],
                    ) as f32)
                    .expect("arrow offset");
                    if actual.to_bits() != expected.to_bits() {
                        failures.push(format!("{name} rate={rate} elapsed={elapsed} beat={beat}: {actual} != {expected}"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn broadcast_params_match_native() {
    fn receive(call: &mut deadlib_lua::HostCall<'_>) -> Result<(), deadlib_lua::Error> {
        let Value::Table(globals) = call.captures()[0] else {
            unreachable!()
        };
        let key = call.try_intern(b"received")?;
        let Value::Number(count) = call.raw_table_get(globals, key)? else {
            panic!("native receiver count")
        };
        call.raw_table_set(globals, key, Value::Number(count + 1.0))?;
        let params = call.arguments().get(1).copied().unwrap_or(Value::Nil);
        let key = call.try_intern(b"received_params")?;
        call.raw_table_set(globals, key, params)
    }
    let mut host = runtime(Path::new("."));
    let declaration = api::eval(host.vm_mut(), "return Def.Actor{}")[0];
    let scene = host
        .instantiate_actor(declaration)
        .expect("message manager receiver");
    let actor = host.actor_scene_root(scene).expect("owned receiver");
    let vm = host.vm_mut();
    let globals = vm.get_global(b"_G").unwrap();
    vm.set_global(b"received", Value::Number(0.0)).unwrap();
    let receive = vm.new_host_function(receive, &[globals]).unwrap();
    api::set(vm, actor, b"ProbeMessageCommand", receive);
    {
        execute(
            host.vm_mut(),
            r#"
            for _, invalid in ipairs({1, true, "text", function() end}) do
                assert(not pcall(function() MESSAGEMAN:Broadcast("Probe", invalid) end))
            end
            assert(received == 0, "invalid parameters must not dispatch")
            local params = { value = 3 }
            assert(MESSAGEMAN:Broadcast("Probe", params) == MESSAGEMAN)
            assert(received == 1 and received_params == params)
            assert(MESSAGEMAN:Broadcast("Probe", nil) == MESSAGEMAN)
            assert(received == 2 and received_params == nil)
            assert(MESSAGEMAN:Broadcast("Probe") == MESSAGEMAN)
            assert(received == 3 and received_params == nil)
        "#,
        );
        Ok::<_, deadlib_lua::Error>(())
    }
    .expect("native Broadcast parameter contract");
}

fn unit_table_set(
    host: &mut SongRuntime,
    owner: Value,
    key: &[u8],
    value: Value,
) -> Result<(), deadlib_lua::Error> {
    api::set(host.vm_mut(), owner, key, value);
    Ok(())
}
fn unit_index_set(
    host: &mut SongRuntime,
    owner: Value,
    index: usize,
    value: Value,
) -> Result<(), deadlib_lua::Error> {
    set_index(host.vm_mut(), owner, index, value);
    Ok(())
}
fn unit_read_sprite_states(
    host: &mut SongRuntime,
    actor: Value,
) -> Result<Vec<deadsync_song_lua::SongLuaSpriteState>, String> {
    test_support::read_sprite_states(host, actor).map_err(|error| error.to_string())
}
fn unit_sync_sprite_states(
    host: &mut SongRuntime,
    actor: Value,
    states: &mut std::sync::Arc<[deadsync_song_lua::SongLuaSpriteState]>,
) -> Result<(), String> {
    test_support::sync_sprite_states(host, actor, states).map_err(|error| error.to_string())
}
fn unit_assert_fresh_sprite(
    host: &mut SongRuntime,
    actor: Value,
    states: &mut std::sync::Arc<[deadsync_song_lua::SongLuaSpriteState]>,
) {
    let expected = unit_read_sprite_states(host, actor).unwrap();
    unit_sync_sprite_states(host, actor, states).unwrap();
    assert_eq!(states.as_ref(), expected);
}
#[test]
fn native_sprite_array_reuses_unchanged_values_and_preserves_old_snapshots() {
    let mut host = runtime(Path::new("."));
    let actor = Ok::<_, deadlib_lua::Error>(
        api::eval(
            host.vm_mut(),
            "return {Frames={{Frame=2,Delay='0.25'},{Delay=0.5}}}",
        )[0],
    )
    .unwrap();
    let mut states: Arc<[_]> = Arc::from([]);
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    let original = states.clone();
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    assert!(Arc::ptr_eq(&original, &states));
    let frames = Ok::<_, deadlib_lua::Error>(api::get(host.vm_mut(), actor, b"Frames")).unwrap();
    {
        let entry = get_index(host.vm_mut(), frames, 1);
        unit_table_set(&mut host, entry, b"Frame", Value::Number(3.0))
    }
    .unwrap();
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    assert_eq!(
        states.iter().map(|state| state.frame).collect::<Vec<_>>(),
        [3, 3]
    );
    assert_eq!(
        original.iter().map(|state| state.frame).collect::<Vec<_>>(),
        [2, 2]
    );
    assert!(!Arc::ptr_eq(&original, &states));
    {
        let value = api::eval(host.vm_mut(), "return {Frame=7}")[0];
        unit_index_set(&mut host, frames, 3, value)
    }
    .unwrap();
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    assert_eq!(states.len(), 3);
    unit_index_set(&mut host, frames, 2, Value::Nil).unwrap();
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    assert_eq!(
        states.len(),
        1,
        "sequence traversal stops at the first hole"
    );
    unit_table_set(
        &mut host,
        actor,
        b"__songlua_state_sprite_default_states",
        Value::Boolean(true),
    )
    .unwrap();
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    assert!(states.is_empty());
    let empty = states.clone();
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    assert!(Arc::ptr_eq(&empty, &states));
    unit_table_set(
        &mut host,
        actor,
        b"__songlua_state_sprite_default_states",
        Value::Boolean(false),
    )
    .unwrap();
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    assert_eq!(states.len(), 1);
    unit_table_set(&mut host, actor, b"Frames", Value::Nil).unwrap();
    {
        let value = api::string(host.vm_mut(), b"9");
        unit_table_set(&mut host, actor, b"Frame0000", value)
    }
    .unwrap();
    unit_table_set(&mut host, actor, b"Delay0000", Value::Number(0.75)).unwrap();
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    assert_eq!(states[0].frame, 9);
    let legacy = states.clone();
    unit_assert_fresh_sprite(&mut host, actor, &mut states);
    assert!(Arc::ptr_eq(&legacy, &states));
}
#[test]
fn native_sprite_publication_visits_getters_once_and_keeps_last_good_array_on_error() {
    let mut host = runtime(Path::new("."));
    let actor = Ok::<_, deadlib_lua::Error>(
        api::eval(
            host.vm_mut(),
            r#"
reads, frame, bad = 0, 4, false
return setmetatable({}, {__index=function(_, key)
    reads=reads+1
    if key=='Frame0000' then return frame end
    if key=='Delay0000' then return '0.5' end
    if key=='Frame0001' and bad then return {} end
end})
"#,
        )[0],
    )
    .unwrap();
    let mut states: Arc<[_]> = Arc::from([]);
    for frame in [4, 4, 8] {
        host.vm_mut()
            .set_global(b"reads", Value::Number(0.0))
            .unwrap();
        host.vm_mut()
            .set_global(b"frame", Value::Number(frame as f64))
            .unwrap();
        let previous = states.clone();
        unit_sync_sprite_states(&mut host, actor, &mut states).unwrap();
        assert_eq!(
            Ok::<_, deadlib_lua::Error>(
                api::as_number(host.vm_mut().get_global(b"reads").unwrap()) as u32
            )
            .unwrap(),
            3
        );
        assert_eq!(states[0].frame, frame);
        if previous.first().is_some_and(|state| state.frame == frame) {
            assert!(Arc::ptr_eq(&previous, &states));
        }
    }
    let previous = states.clone();
    {
        execute(host.vm_mut(), "reads=0; frame=20; bad=true");
        Ok::<_, deadlib_lua::Error>(())
    }
    .unwrap();
    let error = unit_sync_sprite_states(&mut host, actor, &mut states).unwrap_err();
    assert_eq!(
        Ok::<_, deadlib_lua::Error>(
            api::as_number(host.vm_mut().get_global(b"reads").unwrap()) as u32
        )
        .unwrap(),
        3
    );
    assert_eq!(
        error,
        unit_read_sprite_states(&mut host, actor).unwrap_err()
    );
    assert!(
        Arc::ptr_eq(&previous, &states),
        "a partial read must not publish"
    );
}
#[test]
fn native_sprite_publication_compares_delay_bits() {
    let mut host = runtime(Path::new("."));
    let actor = Ok::<_, deadlib_lua::Error>(
        api::eval(host.vm_mut(), "return {Frames={{Frame=0,Delay=0}}}")[0],
    )
    .unwrap();
    let entry = {
        let frames = api::get(host.vm_mut(), actor, b"Frames");
        Ok::<_, deadlib_lua::Error>(get_index(host.vm_mut(), frames, 1))
    }
    .unwrap();
    let mut states: Arc<[_]> = Arc::from([]);
    unit_sync_sprite_states(&mut host, actor, &mut states).unwrap();
    let positive_zero = states.clone();
    unit_table_set(
        &mut host,
        entry,
        b"Delay",
        Value::Number(f64::from(-0.0_f32)),
    )
    .unwrap();
    unit_sync_sprite_states(&mut host, actor, &mut states).unwrap();
    assert_eq!(states[0].delay.to_bits(), (-0.0_f32).to_bits());
    assert!(!Arc::ptr_eq(&positive_zero, &states));
    unit_table_set(&mut host, entry, b"Delay", Value::Number(f64::NAN)).unwrap();
    unit_sync_sprite_states(&mut host, actor, &mut states).unwrap();
    assert!(states[0].delay.is_nan());
    let nan = states.clone();
    unit_sync_sprite_states(&mut host, actor, &mut states).unwrap();
    assert!(
        Arc::ptr_eq(&nan, &states),
        "unchanged NaNs should not force allocations"
    );
}

#[test]
fn date_globals_expose_native_zero_based_day_of_year() {
    let context = SongLuaCompileContext::new(".", "calendar globals");
    let mut host = SongRuntime::new(&context).unwrap();
    for (year, month, day, expected) in [
        (2026, 0, 1, 0),
        (2026, 0, 18, 17),
        (2026, 0, 19, 18),
        (2024, 1, 29, 59),
        (2024, 2, 1, 60),
        (2025, 2, 1, 59),
        (1900, 2, 1, 59),
        (2000, 2, 1, 60),
        (2024, 11, 31, 365),
        (2025, 11, 31, 364),
    ] {
        test_support::install_calendar_for_date(
            &mut host,
            &context,
            deadsync_song_lua::SongLuaDateGlobals {
                year,
                month_of_year: month,
                day_of_month: day,
                hour: 12,
                minute: 0,
                second: 0,
            },
        )
        .expect("calendar globals");
        let getter = host
            .vm_mut()
            .get_global(b"DayOfYear")
            .expect("native DayOfYear calendar getter");
        assert_eq!(
            Ok::<_, deadlib_lua::Error>(api::call_number(host.vm_mut(), getter, &[]) as i32)
                .expect("calendar day"),
            expected,
            "native zero-based day for {year}-{month}-{day}"
        );
    }
}
