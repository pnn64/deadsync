use super::*;

type Prepared = PreparedGameplaySongLua<deadsync_assets::noteskin::SpriteSlot>;

fn fixture() -> (tempfile::TempDir, SongData, crate::SongLuaCompileContext) {
    crate::tests::init_paths();
    let dir = tempfile::tempdir().unwrap();
    for name in ["background.lua", "foreground.lua"] {
        std::fs::write(dir.path().join(name), "return Def.ActorFrame{}").unwrap();
    }
    let simfile = dir.path().join("song.ssc");
    std::fs::write(&simfile, "#TITLE:Lua submission;\n#BPMS:0=120;\n#BGCHANGES:0=background.lua=1=0=0=1;\n#FGCHANGES:0=foreground.lua=1=0=0=1;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n1000\n0000\n0100\n0000\n;\n").unwrap();
    let song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap();
    assert_eq!(song.background_lua_changes.len(), 1);
    assert_eq!(song.foreground_lua_changes.len(), 1);
    let mut context = crate::SongLuaCompileContext::new(dir.path(), &song.title);
    context.music_length_seconds = 0.25;
    (dir, song, context)
}

fn prepare(song: &SongData, context: &crate::SongLuaCompileContext) -> Prepared {
    prepare_song_lua_with_runtime_fallback(
        song,
        context,
        deadsync_assets::song_lua::compile_song_lua,
        deadsync_assets::song_lua::compile_song_lua_layers,
    )
}

#[test]
fn lua_submission_approval_requires_the_current_complete_compilation() {
    let (dir, song, context) = fixture();
    assert!(prepare(&song, &context).compile_succeeded());

    // Lua is outside the chart hash. Reusing the chart must not reuse approval.
    std::fs::write(dir.path().join("foreground.lua"), "return ???").unwrap();
    let failed = prepare(&song, &context);
    assert!(!failed.compile_succeeded());
    assert_eq!(
        failed.background_layers.len(),
        1,
        "visual fallback still works"
    );
    std::fs::write(dir.path().join("foreground.lua"), "return Def.ActorFrame{}").unwrap();
    assert!(prepare(&song, &context).compile_succeeded());

    std::fs::remove_file(dir.path().join("foreground.lua")).unwrap();
    assert!(!prepare(&song, &context).compile_succeeded());
    assert!(!Prepared::default().compile_succeeded());
}

#[test]
fn lua_submission_rejects_captured_callback_errors() {
    let (dir, song, context) = fixture();
    std::fs::write(
        dir.path().join("foreground.lua"),
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function() error("broken authored update") end)
    end
}
"#,
    )
    .unwrap();
    let data = prepare(&song, &context);
    assert!(
        data.primary.is_some(),
        "compiler can retain a partial scene"
    );
    assert!(
        !data.compile_succeeded(),
        "a partial scene cannot approve scores"
    );
}

#[test]
fn lua_submission_allows_a_successful_runtime_compilation() {
    let (dir, song, context) = fixture();
    std::fs::write(
        dir.path().join("foreground.lua"),
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function() return false end)
    end
}
"#,
    )
    .unwrap();
    let mut data = prepare(&song, &context);
    assert!(data.compile_succeeded());
    assert!(data.take_live_scene().unwrap().is_some());
    assert!(data.compile_succeeded());
}

#[test]
fn lua_submission_rejects_a_failed_shared_session_even_if_layers_recover() {
    let (_dir, song, context) = fixture();
    let data: Prepared = prepare_song_lua(
        &song,
        &context,
        deadsync_assets::song_lua::compile_song_lua,
        |_, _, _| Err("shared scene failed".into()),
    );
    assert!(data.primary.is_some());
    assert_eq!(data.background_layers.len(), 1);
    assert!(!data.compile_succeeded());
}
