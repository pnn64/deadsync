use super::*;

fn data() -> noteskin_itg::NoteskinData {
    noteskin_itg::NoteskinData {
        name: "callback-fixture".into(),
        overrides: vec![],
        metrics: Default::default(),
        search_dirs: vec![Path::new("").to_owned()],
    }
}

fn installed() -> Session {
    let mut session = Session::new().unwrap();
    session.install("dance", &data(), "Mirror").unwrap();
    session
}

fn run(session: &mut Session, source: &str) -> LuaValue {
    let function = session.compile(source, "=noteskin regression").unwrap();
    session.call(function, &[]).unwrap()
}

#[test]
fn ignored_callbacks_preserve_values_identities_argument_effects_and_coroutines() {
    let mut session = installed();
    run(
        &mut session,
        r#"
        local calls = 0
        local function args()
            calls = calls + 1
            collectgarbage()
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
        local thread = coroutine.create(function()
            assert(game:GetName(args()) == 'dance')
            assert(state:GetPlayerOptionsString(args()) == 'Mirror')
        end)
        assert(coroutine.resume(thread))
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

#[test]
fn command_extraction_preserves_holes_coercions_errors_and_callback_effects() {
    check_command_extraction(true);
}

fn check_command_extraction(collect: bool) {
    let mut session = installed();
    for (commands, expected) in [
        ("{}", Ok(None)),
        (
            "{'', '  ', 'x,1', '', ' y,2 ', 3, 'z,4'}",
            Ok(Some("x,1; y,2 ;3;z,4")),
        ),
        ("{[1]='x,1',[3]='z,3'}", Ok(Some("x,1"))),
        ("{'x,1', false, 'z,3'}", Err(())),
        ("{'x,1', '\\255'}", Err(())),
        ("{'x,1', '\\000', ' y,2 '}", Ok(Some("x,1;\0; y,2 "))),
        (
            "setmetatable({'x,1'}, {__index=function() error('index hook') end})",
            Ok(Some("x,1")),
        ),
    ] {
        let source = format!("return {{__loader_commands={commands}}}");
        let actor = run(&mut session, &source);
        let actual = session
            .read_entry("Down", "Tap Note", actor)
            .map(|entry| entry.init_command);
        let actual = actual
            .as_ref()
            .map(|value| value.as_deref())
            .map_err(|_| ());
        assert_eq!(actual, expected, "{commands}");
    }
    let actor = run(
        &mut session,
        if collect {
            "return Def.Sprite{InitCommand=function(self) collectgarbage(); self:x(1):y(2) end}"
        } else {
            "return Def.Sprite{InitCommand=function(self) self:x(1):y(2) end}"
        },
    );
    for expected in ["x,1;y,2", "x,1;y,2;x,1;y,2"] {
        assert_eq!(
            session
                .read_entry("Down", "Tap Note", actor)
                .unwrap()
                .init_command
                .as_deref(),
            Some(expected)
        );
    }
    for command in ["", "  ", "x,1", " x,1 ; y,2 "] {
        let actor = session.table().unwrap();
        session.put_text(actor, "InitCommand", command).unwrap();
        let expected = if command.trim().is_empty() {
            None
        } else {
            Some(command)
        };
        assert_eq!(
            session
                .read_entry("Down", "Tap Note", actor)
                .unwrap()
                .init_command
                .as_deref(),
            expected
        );
    }
}

#[test]
fn aliases_preserve_identity_raw_order_and_metamethod_effects_across_gc() {
    check_aliases(true);
}

fn check_aliases(collect: bool) {
    let mut session = installed();
    let source = r#"
        local values = {left={}, LEFT={}, down=false, [7]=99, ['\255']=17}
        local expected
        for key, value in pairs(values) do
            if type(key)=='string' and string.lower(key)=='left' then expected=value; break end
        end
        local calls = {}
        setmetatable(values, {
            __index=function(_, key)
                collectgarbage(); calls[#calls+1]='get:'..key
                if key=='Missing' then return false end
            end,
            __newindex=function(t,key,value)
                collectgarbage(); calls[#calls+1]='set:'..key; rawset(t,key,value)
            end
        })
        result={Rotate=values, expected=expected, calls=calls}
        return result
    "#;
    let source = if collect {
        source.to_owned()
    } else {
        source.replace("collectgarbage();", "")
    };
    let noteskin = run(&mut session, &source);
    let names = ["Left", "Down", "Missing", "Unseen"].map(String::from);
    session.normalize(noteskin, &names, &[]).unwrap();
    run(
        &mut session,
        r#"
        assert(rawequal(rawget(result.Rotate,'Left'), result.expected))
        assert(rawget(result.Rotate,'Down') == false)
        assert(rawget(result.Rotate,'Missing') == nil)
        assert(rawget(result.Rotate,'Unseen') == nil)
        assert(table.concat(result.calls, ',') == 'get:Left,set:Left,get:Down,set:Down,get:Missing,get:Unseen')
    "#,
    );
}

#[test]
fn safe_libraries_and_module_failures_match_the_production_baseline() {
    let source = r#"
        assert(debug == nil and package.loaded.debug == nil)
        assert(type(io)=='table' and type(os)=='table' and type(coroutine)=='table')
        local ok, message = pcall(package.loadlib, 'missing-library', 'entry')
        assert(not ok and string.find(tostring(message),'package.loadlib is disabled in safe mode',1,true))
        assert(#package.loaders == 3)
        assert(package.loaders[3]() == "\n\tcan't load C modules in safe mode")
        local state = {}
        package.preload.fixture = function() collectgarbage(); return state end
        assert(require('fixture') == state and require('fixture') == state)
    "#;
    run(&mut installed(), source);
}

#[test]
fn complete_compiled_callback_fixture_bytes_match_the_frozen_baseline() {
    let directory = crate::resource_perf_support::Directory::new("rust-callback-parity");
    let mut data = data();
    data.search_dirs = vec![directory.0.clone()];
    fs::write(
        directory.0.join("NoteSkin.lua"),
        r#"
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
    "#,
    )
    .unwrap();
    let new = compile_data("dance", &data, "source", "Mirror").unwrap();
    assert_eq!((new.loader.version, new.actors.version), (30, 30));
    let bytes = bincode::encode_to_vec(&new, bincode::config::standard()).unwrap();
    assert_eq!(
        bytes,
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/callback-version-30.bin"
        ))
        .as_slice()
    );
    assert_eq!(
        new.loader.entries.len(),
        4 * crate::compiler::CORE_ELEMENTS.len()
    );
}

#[cfg(panic = "unwind")]
#[test]
fn host_panic_propagates_to_rust_and_session_destruction_releases_captures() {
    struct Dropped(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut session = installed();
        let callback = session
            .vm
            .new_host_function_with_data(
                |_| panic!("noteskin host panic"),
                &[],
                Dropped(dropped.clone()),
            )
            .unwrap();
        session.keep(callback).unwrap();
        session.put(session.globals, "panic_arg", callback).unwrap();
        // Rust host panics unwind through the VM; Lua pcall cannot swallow them.
        // The old C bridge's panic-as-userdata transport remains oracle-only.
        run(
            &mut session,
            "local _, payload=pcall(panic_arg); return cmd(payload)",
        );
    }))
    .unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>().copied(),
        Some("noteskin host panic")
    );
    assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
}

#[path = "../compiler_aliases_perf.rs"]
mod aliases_perf;
#[path = "../compiler_callbacks_perf.rs"]
mod callbacks_perf;
