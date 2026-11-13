use super::*;
use crate::test_directory::Directory;
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
    let directory = Directory::new("callback-parity");
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
