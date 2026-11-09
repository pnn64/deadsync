//! Original option binding microfixtures driven by the Rust interpreter.
use deadlib_lua::{Error, Value, compile_named};
use deadsync_song_lua::{
    *, {SongRuntime, test_support},
};
use std::path::Path;
#[path = "support/baked_program.rs"]
mod api;
fn option_exec(host: &mut SongRuntime, source: impl AsRef<str>) -> Result<Vec<Value>, Error> {
    let code = compile_named(source.as_ref().as_bytes(), b"=original option unit fixture")?;
    host.vm_mut().execute(&code).map(|values| values.to_vec())
}
fn option_fixture(context: &SongLuaCompileContext) -> SongRuntime {
    let mut host = SongRuntime::new(context).unwrap();
    test_support::expose_catalog(&mut host).unwrap();
    option_exec(
        &mut host,
        r#"
player=GAMESTATE:GetPlayerState(0)
o=player:GetPlayerOptions('ModsLevel_Song');options=o
NOTESKIN={DoesNoteSkinExist=function()return false end,GetNoteSkinNames=function()return{}end}
"#,
    )
    .unwrap();
    host
}
fn option_value(host: &mut SongRuntime) -> Value {
    host.vm_mut().get_global(b"o").unwrap()
}
fn install_option_control_adapter(host: &mut SongRuntime) {
    option_exec(
        host,
        r#"
function _ITG_OPTIONS_LEVEL(value)
 local chosen=GAMESTATE:GetSongOptionsObject(value)
 for _,name in ipairs{'Preferred','Stage','Song','Current'} do
  if chosen==GAMESTATE:GetSongOptionsObject(name)then return 'ModsLevel_'..name end
 end
end
local groups=catalog.option_operations
function _ITG_OPTIONS_ADVANCE(seconds,scaled)groups:Advance(seconds)end
"#,
    )
    .unwrap();
}
fn option_function(host: &mut SongRuntime, options: Value, name: &str) -> Value {
    let vm = host.vm_mut();
    let lookup = api::eval(vm, b"return function(owner, key) return owner[key] end")[0];
    let key = api::string(vm, name.as_bytes());
    api::call_one(vm, lookup, &[options, key])
}
fn option_raw(host: &mut SongRuntime, owner: Value, key: &str) -> Value {
    let vm = host.vm_mut();
    let Value::Table(table) = owner else {
        panic!("option fixture table")
    };
    let key = vm.try_intern(key.as_bytes()).unwrap();
    vm.raw_table_get(table, key).unwrap()
}
fn option_state_value(host: &mut SongRuntime, options: Value, key: &str) -> Result<Value, Error> {
    let state = option_raw(host, options, "__songlua_player_option_state");
    Ok(option_raw(host, state, key))
}
fn option_speed(host: &mut SongRuntime, options: Value, key: &str) -> Result<Option<f32>, Error> {
    let speeds = option_raw(host, options, "__songlua_player_option_speeds");
    if speeds == Value::Nil {
        return Ok(None);
    }
    match option_raw(host, speeds, key) {
        Value::Nil => Ok(None),
        Value::Number(value) => Ok(Some(value as f32)),
        _ => host
            .vm_mut()
            .with_native_stack(|stack| Err(stack.error(b"numeric option speed expected"))),
    }
}
fn option_call_one(
    host: &mut SongRuntime,
    function: Value,
    args: &[Value],
) -> Result<Value, Error> {
    Ok(host
        .vm_mut()
        .call(function, args)?
        .first()
        .copied()
        .unwrap_or(Value::Nil))
}
fn option_query_number(
    host: &mut SongRuntime,
    function: Value,
    owner: Value,
    index: f64,
) -> Result<f32, Error> {
    let value = option_call_one(host, function, &[owner, Value::Number(index)])?;
    match value {
        Value::Number(value) => Ok(value as f32),
        _ => host
            .vm_mut()
            .with_native_stack(|stack| Err(stack.error(b"numeric option result expected"))),
    }
}
fn option_query_optional(
    host: &mut SongRuntime,
    function: Value,
    owner: Value,
    index: f64,
) -> Result<Option<f32>, Error> {
    let value = option_call_one(host, function, &[owner, Value::Number(index)])?;
    match value {
        Value::Nil => Ok(None),
        Value::Number(value) => Ok(Some(value as f32)),
        _ => host.vm_mut().with_native_stack(|stack| {
            Err(stack.error(b"optional numeric option result expected"))
        }),
    }
}

#[test]
fn stage_modifiers_preserve_options_and_native_player_enum() {
    let context = SongLuaCompileContext::new(".", "stage modifiers");
    let mut host = option_fixture(&context);
    option_exec(
        &mut host,
        r#"
            local p1 = GAMESTATE:GetPlayerState('PlayerNumber_P1')
            local p2 = GAMESTATE:GetPlayerState('PlayerNumber_P2')
            p2:GetPlayerOptions('ModsLevel_Stage'):Mini(0.25)
            GAMESTATE:GetSongOptionsObject('ModsLevel_Stage'):AssistClap(true)
            assert(GAMESTATE:ApplyStageModifiers(1, '50% Reverse, 1.5xmusic') == GAMESTATE)
            for _, level in ipairs({'ModsLevel_Stage', 'ModsLevel_Song', 'ModsLevel_Current'}) do
                assert(p2:GetPlayerOptions(level):Reverse() == 0.5)
                assert(p1:GetPlayerOptions(level):Reverse() == 0)
                assert(GAMESTATE:GetSongOptionsObject(level):MusicRate() == 1.5)
            end
            assert(p2:GetPlayerOptions('ModsLevel_Stage'):Mini() == 0.25)
            assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Stage'):AssistClap())
            assert(p2:GetPlayerOptions('ModsLevel_Preferred'):Reverse() == 0)
            assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Preferred'):MusicRate() == 1)
            GAMESTATE:ApplyStageModifiers('PlayerNumber_P1', '25% Reverse')
            assert(p1:GetPlayerOptions('ModsLevel_Current'):Reverse() == 0.25)
            assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate() == 1.5)
            assert(not pcall(function() GAMESTATE:ApplyStageModifiers(2, 'Reverse') end))
        "#,
    )
    .expect("native stage modifier propagation");
}

#[test]
fn reverse_columns_match_native_composition() {
    for (name, expected) in [
        ("single", &[0.5, 0.625, 1.0, 0.875][..]),
        (
            "double",
            &[0.5, 0.375, 0.5, 0.625, 1.0, 0.875, 0.75, 0.875][..],
        ),
    ] {
        let mut context = SongLuaCompileContext::new(Path::new("."), "option unit fixture");
        context.style_name = name.into();
        let mut host = option_fixture(&context);
        let options = option_value(&mut host);
        option_exec(
            &mut host,
            "o:FromString('25% Reverse, 50% Split, 12.5% Alternate, 25% Cross, 25% Reverse1')",
        )
        .expect("write scroll options");
        let query = option_function(&mut host, options, "GetReversePercentForColumn");
        for (column, &expected) in expected.iter().enumerate() {
            assert_eq!(
                option_query_number(&mut host, query, options, column as f64)
                    .expect("column reverse"),
                expected,
                "{name} column {column}"
            );
        }
        assert!(
            option_query_optional(&mut host, query, options, (-1) as f64)
                .expect("negative column")
                .is_none()
        );
        assert!(
            option_query_optional(&mut host, query, options, (expected.len() + 1) as f64)
                .expect("overflow column")
                .is_none()
        );
        option_exec(&mut host,"o:FromString('clearall, 175% Reverse'); assert(o:GetReversePercentForColumn(0) == 0.25); o:FromString('clearall, -50% Reverse'); assert(o:GetReversePercentForColumn(0) == -0.5)").expect("native folding");
    }
}

#[test]
fn perspective_aliases_read_shared_native_fields_and_speeds() {
    let mut host = option_fixture(&SongLuaCompileContext::new(
        Path::new("."),
        "option unit fixture",
    ));
    let options = option_value(&mut host);
    option_exec(
        &mut host,
        r#"
assert(o:Overhead() == true and select('#', o:Overhead()) == 1)
for _, name in ipairs{'Incoming', 'Space', 'Hallway', 'Distant'} do
    local value, speed = o[name](o)
    assert(value == nil and speed == nil and select('#', o[name](o)) == 2)
end
o:FromString('*3 50% incoming')
assert(o:Tilt() == -0.5 and o:Skew() == 0.5 and o:Overhead() == false)
local value, speed = o:Incoming()
assert(value == 0.5 and speed == 3)
local old, oldspeed = o:Space(-0.25, 7)
assert(old == nil and oldspeed == nil and o:Space() == -0.25)
assert(o:Incoming() == nil and o:Tilt() == -0.25 and o:Skew() == -0.25)
old, oldspeed = o:Space(0.75)
assert(old == -0.25 and oldspeed == 7)
o:Hallway(-0.75)
assert(o:Hallway() == nil and o:Distant() == 0.75 and o:Skew() == 0)
old, oldspeed = o:Tilt(-0.4, 2)
assert(old == 0.75 and oldspeed == 7 and math.abs(o:Hallway() - 0.4) < 1e-6)
old, oldspeed = o:Skew(0.6, 9)
assert(old == 0 and oldspeed == 7)
value, speed = o:Incoming()
assert(math.abs(value - 0.6) < 1e-6 and speed == 9)
assert(o:Space() == nil and o:Hallway() == nil and o:Distant() == nil)
o:Incoming('-0.5', '4')
assert(o:Incoming() == -0.5 and o:Tilt() == 0.5 and o:Skew() == -0.5)
o:Distant(-0.25)
assert(o:Hallway() == 0.25 and o:Distant() == nil)
"#,
    )
    .expect("native perspective aliases");
    let state = options;
    for alias in ["incoming", "space", "hallway", "distant", "overhead"] {
        assert!(matches!(
            option_state_value(&mut host, state, alias).expect("alias state"),
            Value::Nil
        ));
    }
}

#[test]
fn timer_binding_matches_native_enum_and_float_protocol() {
    let mut host = option_fixture(&SongLuaCompileContext::new(
        Path::new("."),
        "option unit fixture",
    ));
    let _options = option_value(&mut host);
    option_exec(
        &mut host,
        r#"
assert(o:ModTimerSetting() == 'ModTimerType_Default' and select('#', o:ModTimerSetting()) == 1)
assert(o:ModTimerSetting('ModTimerType_Song') == 'ModTimerType_Default')
assert(o:ModTimerSetting('GaMe', true) == o and o:ModTimerSetting() == 'ModTimerType_Game')
assert(o:ModTimerSetting(1) == 'ModTimerType_Game' and o:ModTimerSetting() == 'ModTimerType_Beat')
assert(o:ModTimerSetting(nil) == 'ModTimerType_Beat' and o:ModTimerSetting(nil, true) == o)
for _, invalid in ipairs({-1, 4, 1.5, '1', 'modtimertype_song', false}) do
    assert(not pcall(function() o:ModTimerSetting(invalid) end))
    assert(o:ModTimerSetting() == 'ModTimerType_Beat')
end
for _, mode in ipairs({'Game', 'Beat', 'Song', 'Default'}) do
    o:FromString('*0 no modtimer'..mode)
    assert(o:ModTimerSetting() == 'ModTimerType_'..mode)
end
for _, name in ipairs({'ModTimerMult', 'ModTimerOffset'}) do
    local value, speed = o[name](o)
    assert(value == 0 and speed == 1 and select('#', o[name](o)) == 2)
    assert(o[name](o, -2, 3, true) == o)
    value, speed = o[name](o, 1)
    assert(value == -2 and speed == 3)
    assert(not pcall(function() o[name](o, -.5, -1) end))
    assert(o[name](o) == -.5 and select(2, o[name](o)) == 3)
end
o:FromString('modtimersong, clearall')
assert(o:ModTimerSetting() == 'ModTimerType_Default')
assert(o:ModTimerMult() == 0 and select(2, o:ModTimerMult()) == 1)
assert(o:ModTimerOffset() == 0 and select(2, o:ModTimerOffset()) == 1)
"#,
    )
    .expect("native timer protocol");
}

#[test]
fn perspective_methods_keep_native_truthiness_chaining_and_error_order() {
    let mut host = option_fixture(&SongLuaCompileContext::new(
        Path::new("."),
        "option unit fixture",
    ));
    let _options = option_value(&mut host);
    option_exec(
        &mut host,
        r#"
assert(o:Space(0.5, 3, true) == o)
assert(o:Overhead(false, 4, true) == o)
assert(o:Tilt() == 0.5 and o:Skew() == 0.5)
local value, speed = o:Space()
assert(value == 0.5 and speed == 4)
assert(o:Overhead(false, 6) == false and o:Overhead() == false)
assert(select(2, o:Tilt()) == 6 and select(2, o:Skew()) == 6)
assert(o:Overhead(0) == false and o:Overhead() == true)
assert(o:Overhead(true) == o)
assert(o:Space(0.75, 4, true):Incoming(-0.5, true) == o)
value, speed = o:Incoming(nil, 11)
assert(value == -0.5 and speed == 4)
assert(o:Incoming() == -0.5 and select(2, o:Tilt()) == 11 and select(2, o:Skew()) == 11)
assert(o:Hallway(true) == o and o:Incoming() == -0.5)
assert(not pcall(o.Hallway, o, 0.75, -1))
assert(o:Hallway() == 0.75 and select(2, o:Hallway()) == 11)
assert(o:Overhead(nil, 8, true) == o and o:Hallway() == 0.75)
assert(select(2, o:Tilt()) == 8 and select(2, o:Skew()) == 8)
o:FromString('no overhead')
assert(o:Overhead() == true)
o:FromString('*3 50% incoming,*7 -25% space,*4 no distant')
assert(o:Overhead() == true and select(2, o:Tilt()) == 4 and select(2, o:Skew()) == 4)
"#,
    )
    .expect("native perspective calls");
}

#[test]
fn perspective_getters_select_one_speed_correction_in_any_order() {
    let mut host = option_fixture(&SongLuaCompileContext::new(
        Path::new("."),
        "option unit fixture",
    ));
    let _options = option_value(&mut host);
    option_exec(&mut host,r#"
-- Sharkmode queries all four aliases in a pairs loop. Only the active alias
-- may be truthy; otherwise hash iteration changes the chosen correction.
for _, order in ipairs{{'Hallway','Distant','Incoming','Space'}, {'Space','Incoming','Distant','Hallway'}} do
    for _, active in ipairs(order) do
        o[active](o, 0.5, 3)
        local selected, count = nil, 0
        for _, name in ipairs(order) do
            if o[name](o) then selected = name; count = count + 1 end
        end
        assert(selected == active and count == 1)
    end
end
"#).expect("stable perspective selection");
}

#[test]
fn repeated_state_option_replacement_releases_temporary_values() {
    let context = SongLuaCompileContext::new(Path::new("."), "Repeated option assignment");
    let mut host = option_fixture(&context);
    let _options = option_value(&mut host);
    option_exec(
        &mut host,
        r#"
local options = player:GetPlayerOptions("ModsLevel_Song")
for i = 1, 1000 do
    player:SetPlayerOptions("ModsLevel_Song", "*3 75% Reverse, C500")
    assert(options:Reverse() == 0.75 and options:CMod() == 500)
    player:SetPlayerOptions("ModsLevel_Song", "")
    assert(options:Reverse() == 0 and options:XMod() == 1 and options:CMod() == nil)
    if i % 10 == 0 then collectgarbage("collect") end
end
"#,
    )
    .expect("temporary native option values must not retain Lua references");
}

#[test]
fn native_song_seconds_use_resolved_note_bounds_and_absolute_hint() {
    let mut context = SongLuaCompileContext::new(Path::new("."), "Native song bounds");
    context.music_length_seconds = 120.0;
    context.song_note_bounds = Some([10.0, 90.0]);
    context.song_music_rate = 1.5;
    context.global_offset_seconds = 0.125;
    let mut host = option_fixture(&context);
    option_exec(
        &mut host,
        r#"
local song = GAMESTATE:GetCurrentSong()
assert(song:MusicLengthSeconds() == 120)
assert(song:GetFirstSecond() == 10 and song:GetLastSecond() == 90)
GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate(2)
assert(song:GetFirstSecond() == 10 and song:GetLastSecond() == 90)
assert(song:GetLastSecond() - song:GetFirstSecond() == 80)
"#,
    )
    .expect("capture-engine endpoints remain resolved when the current music rate changes");
}

#[test]
fn state_options_replace_previous_targets() {
    let context = SongLuaCompileContext::new(Path::new("."), "Option assignment");
    let mut host = option_fixture(&context);
    let options = option_value(&mut host);
    option_exec(
        &mut host,
        r#"
local options = player:GetPlayerOptions("ModsLevel_Song")
options:FromString("*7 50% Drunk, Shuffle, C500, *2 75% Space")
options:FromString("25% Mini")
assert(options:Drunk() == 0.5 and options:Mini() == 0.25)
assert(options:Space() == 0.75 and options:Overhead() == false)
player:SetPlayerOptions("ModsLevel_Song", "*3 75% Reverse")
assert(options == player:GetPlayerOptions("ModsLevel_Song"))
assert(options:Drunk() == 0 and options:Mini() == 0 and not options:Shuffle())
assert(options:Overhead() == true and options:Space() == nil)
assert(options:XMod() == 1 and options:CMod() == nil and options:Reverse() == 0.75)
player:SetPlayerOptions("ModsLevel_Song", "")
assert(options:Reverse() == 0 and options:XMod() == 1)
"#,
    )
    .expect("run native-style option assignment");
    let speeds = options;
    for key in ["drunk", "reverse", "xmod", "cmod", "mmod", "tilt", "skew"] {
        assert_eq!(
            option_speed(&mut host, speeds, key)
                .map(|value| value.unwrap_or(1.0))
                .expect("recorded approach"),
            1.0
        );
    }
}

#[test]
fn current_levels_match_compiled_native_control() {
    let context = SongLuaCompileContext::new(Path::new("."), "Native option levels");
    let mut host = option_fixture(&context);
    install_option_control_adapter(&mut host);
    // Adapt the already-verified C++ control to normal Lua APIs. Its
    // assertion body is byte-identical to the standalone harness fixture;
    // no expected numbers or approach arithmetic are generated here.
    option_exec(
        &mut host,
        r#"
function _ITG_OPTIONS_AT(player, level, method, ...)
    if player == -1 then
        if method == 'SetSongOptions' then return GAMESTATE:SetSongOptions(level, ...) end
        local options = GAMESTATE:GetSongOptionsObject(level)
        return options[method](options, ...)
    end
    local state = GAMESTATE:GetPlayerState(player)
    if method == 'SetPlayerOptions' then return state:SetPlayerOptions(level, ...) end
    local options = state:GetPlayerOptions(level)
    return options[method](options, ...)
end
function _ITG_USING_MODIFIER(player, text) return GAMESTATE:PlayerIsUsingModifier(player, text) end
function _ITG_OPTIONS_RATE_TWEENS(value)
    local previous = PREFSMAN:GetPreference('RateModsAffectFGChanges')
    if type(value) == 'boolean' then PREFSMAN:SetPreference('RateModsAffectFGChanges', value) end
    return previous
end
"#,
    )
    .expect("public option API adapter");
    for source in [
        include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/current-options/window-control.json"
        ),
        include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/current-options/native-control.json"
        ),
        include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/current-options/speed-query-control.json"
        ),
    ] {
        let control: serde_json::Value =
            serde_json::from_str(source).expect("standalone compiled-native control");
        option_exec(
            &mut host,
            control["lua_assertions"]
                .as_str()
                .expect("native assertions"),
        )
        .expect("compiled native option level and clock expectations");
    }
}

#[test]
fn modifier_prefix_order_matches_native() {
    let mut host = option_fixture(&SongLuaCompileContext::new(
        Path::new("."),
        "option unit fixture",
    ));
    let _options = option_value(&mut host);
    option_exec(
        &mut host,
        r#"
o:Tipsy(7)
o:FromString('0.8 0% Tipsy')
assert(o:Tipsy() == 0)
o:FromString('*2 10% *5 25% Drunk')
assert(o:Drunk() == 0.25 and select(2, o:Drunk()) == 5)
o:FromString('50% no Flip')
assert(o:Flip() == 0)
o:FromString('no 75% Flip')
assert(o:Flip() == 0.75)
o:FromString('*inf 25% Mini')
assert(o:Mini() == 0.25 and select(2, o:Mini()) == 1)
o:FromString('*-2 25% Mini')
assert(select(2, o:Mini()) == -2)
o:FromString('100ms Passmark')
assert(math.abs(o:Passmark() - 0.1) < 1e-6)
o:FromString('25* Drunk')
assert(o:Drunk() == 0.25)
o:FromString('.5 Drunk')
assert(o:Drunk() == 1)
"#,
    )
    .expect("native modifier prefixes");
}

#[test]
fn speed_option_writes_preserve_shared_approach_speed() {
    let mut host = option_fixture(&SongLuaCompileContext::new(
        Path::new("."),
        "option unit fixture",
    ));
    let options = option_value(&mut host);
    for (script, expected) in [
        ("options:XMod(2.5)", [1.0, 1.0, 1.0, 1.0]),
        ("options:XMod(2.5, 10000)", [10000.0; 4]),
        ("options:CMod(300)", [10000.0; 4]),
        ("options:MMod(400, 0)", [0.0; 4]),
        ("options:XMod(1.5)", [0.0; 4]),
        ("options:FromString('*7 2x')", [7.0, 0.0, 0.0, 7.0]),
        ("options:FromString('C500')", [7.0, 1.0, 0.0, 1.0]),
    ] {
        option_exec(&mut host, script).unwrap();
        let speeds = options;
        for (key, expected) in ["xmod", "cmod", "mmod", "_spacing"]
            .into_iter()
            .zip(expected)
        {
            assert_eq!(
                // Missing capture metadata is the implicit native Init
                // speed of one, not an authored modifier target.
                option_speed(&mut host, speeds, key).unwrap().unwrap_or(1.0),
                expected,
                "{script}: {key}"
            );
        }
    }
}

#[test]
fn boolean_options_match_native_binding() {
    let methods = SONG_LUA_PLAYER_OPTION_CAPABILITIES
        .iter()
        .copied()
        .filter(|name| *name != "Overhead" && player_option_uses_bool(&name.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    assert_eq!(methods.len(), 40, "cover every native BOOL_INTERFACE");
    for method in methods {
        let key = method.to_ascii_lowercase();
        let mut host = option_fixture(&SongLuaCompileContext::new(
            Path::new("."),
            "option unit fixture",
        ));
        let options = option_value(&mut host);
        option_exec(
            &mut host,
            r#"
assert(o:StealthType() == false and select('#', o:StealthType()) == 1)
assert(o:StealthType(true) == false and o:StealthType() == true)
for _, value in ipairs({0, 1, 'true', 'false', {}}) do
    assert(o:StealthType(value) == true and o:StealthType() == true)
end
assert(o:StealthType(nil, false) == o)
assert(o:StealthType(false, 0, true) == true and o:StealthType() == false)
assert(o:StealthType(true, true) == o)
"#
            .replace("StealthType", method),
        )
        .expect("native boolean protocol");
        // Direct boolean writes have no approach speed, even with a numeric arg.
        let speeds = options;
        option_exec(&mut host, format!("o:{method}(true, -1)")).expect("numeric arg is ignored");
        assert_eq!(
            option_speed(&mut host, speeds, &key).expect("no approach speed"),
            None
        );
    }
}

#[test]
fn boolean_option_strings_use_native_half_threshold() {
    let mut host = option_fixture(&SongLuaCompileContext::new(
        Path::new("."),
        "option unit fixture",
    ));
    let _options = option_value(&mut host);
    option_exec(
        &mut host,
        r#"
options:FromString('50% stealthpastreceptors')
assert(options:StealthPastReceptors() == false)
options:FromString('51% stealthpastreceptors')
assert(options:StealthPastReceptors() == true)
options:FromString('-50% stealthpastreceptors')
assert(options:StealthPastReceptors() == false)
options:FromString('25% noholds')
assert(options:NoHolds() == false)
options:FromString('51% noholds')
assert(options:NoHolds() == true)
"#,
    )
    .expect("native boolean threshold");
}

#[test]
fn dynamic_player_option_methods_are_cached_after_first_lookup() {
    let mut host = option_fixture(&SongLuaCompileContext::new(
        Path::new("."),
        "option unit fixture",
    ));
    let options = option_value(&mut host);
    assert!((option_raw(&mut host, options, "Drunk") == Value::Nil));

    let first = option_function(&mut host, options, "Drunk");
    let cached = option_raw(&mut host, options, "Drunk");
    assert_eq!(first, cached);

    option_call_one(
        &mut host,
        first,
        &[options, Value::Number(f64::from(0.5_f32))],
    )
    .unwrap();
    assert_eq!(
        option_state_value(&mut host, options, "drunk")
            .map(api::as_number)
            .map(|value| value as f32)
            .unwrap(),
        0.5
    );
}

#[test]
fn normalizes_player_option_values() {
    let mut host = option_fixture(&SongLuaCompileContext::new(
        ".",
        "option value normalization",
    ));
    let value =
        option_exec(&mut host, "o:FromString('100% NoMines');return o:NoMines()").unwrap()[0];
    assert_eq!(value, Value::Boolean(true));
    // The old private default used enum ordinal0. The native public getter
    // exposes that same value by its canonical enum name.
    let value = option_exec(&mut host, "return o:FailSetting()").unwrap()[0];
    assert_eq!(api::bytes(host.vm_mut(), value), b"FailType_Immediate");
    let value = option_exec(&mut host, "o:Reverse(0.5);return o:Reverse()").unwrap()[0];
    assert_eq!(value, Value::Number(0.5));
}
#[test]
fn speedmod_value_matches_requested_variant() {
    let mut host = option_fixture(&SongLuaCompileContext::new(".", "speedmod variant"));
    let value = option_exec(&mut host, "o:CMod(400);return o:CMod()").unwrap()[0];
    assert_eq!(value, Value::Number(400.0));
    let value = option_exec(&mut host, "o:XMod(1.5);return o:CMod()").unwrap()[0];
    assert_eq!(value, Value::Nil);
}
fn selected_window(host: &mut SongRuntime, select: Value, value: Value) -> Option<String> {
    let value = api::call_one(host.vm_mut(), select, &[value]);
    if value == Value::Nil {
        None
    } else {
        Some(String::from_utf8(api::bytes(host.vm_mut(), value)).unwrap())
    }
}
#[test]
fn timing_window_name_accepts_numbers_and_names() {
    let mut host = option_fixture(&SongLuaCompileContext::new(".", "timing window names"));
    let select = option_exec(
        &mut host,
        r#"return function(value)
 o:ResetDisabledTimingWindows()
 local ok,err=pcall(function() o:DisableTimingWindow(value) end)
 if not ok then assert(type(err)=='string' and err:find('TimingWindow',1,true));return nil end
 return next(rawget(o,'__songlua_disabled_timing_windows'))
end"#,
    )
    .unwrap()[0];
    host.vm_mut().pin(select).unwrap();
    assert_eq!(
        selected_window(&mut host, select, Value::Number(1.)).as_deref(),
        Some("TimingWindow_W2")
    );
    let name = api::string(host.vm_mut(), "TimingWindow_W5");
    assert_eq!(
        selected_window(&mut host, select, name).as_deref(),
        Some("TimingWindow_W5")
    );
    assert_eq!(
        selected_window(&mut host, select, Value::Number(0.)).as_deref(),
        Some("TimingWindow_W1")
    );
    assert_eq!(
        selected_window(&mut host, select, Value::Number(8.)).as_deref(),
        Some("TimingWindow_Roll")
    );
    assert_eq!(selected_window(&mut host, select, Value::Number(10.)), None);
}
