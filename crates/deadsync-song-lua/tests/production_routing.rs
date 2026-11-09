//! Public compiler entry points share the production runtime.
use deadsync_song_lua::{self as song, *};
use std::{path::Path, sync::Arc};
type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;

fn compile(
    source: &str,
    live: bool,
) -> (
    CompiledSongLua<SongLuaOverlayActor<Kind>>,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.lua");
    std::fs::write(&path, source).unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "Production Rust routing");
    context.live = live;
    context.music_length_seconds = 0.1;
    let output = compile_song_lua(
        &path,
        &context,
        Default::default(),
        |_: &Path, _: &Path, _: &Path| Ok(Arc::<[()]>::from([])),
        |_: &()| None,
        |_, _| None::<(Kind, SongLuaOverlayState)>,
    )
    .unwrap();
    (output, dir)
}

#[test]
fn preparation_and_live_public_entry_points_use_the_runtime() {
    let source = "return Def.Quad{InitCommand=function(self) self:xy(7,9):zoomto(3,4) end}";
    let (prepared, _dir) = compile(source, false);
    assert!(prepared.live_scene.is_none());
    assert_eq!(prepared.overlays.len(), 1);
    assert_eq!(prepared.overlays[0].initial_state.x, 7.);
    let (mut live, _dir) = compile(source, true);
    let mut scene = live.live_scene.take().unwrap();
    fn require_runtime_scene(_: &song::SongLiveScene) {}
    require_runtime_scene(&scene);
    assert_eq!(scene.states()[0].x, 7.);
    assert_eq!(scene.states()[0].y, 9.);
    assert!(
        scene
            .update(SongLuaLiveFrame::default())
            .unwrap()
            .errors
            .is_empty()
    );
    scene.end().unwrap();
}

#[test]
fn product_identity_is_available_during_preparation_and_live_commands() {
    let source = r#"
        local products = {ProductFamily, ProductID, ProductVersion}
        local expected = {'ITGmania', 'ITGmania', '1.2.0'}
        ProductFamily, ProductID, ProductVersion = nil, nil, nil
        local function check()
            collectgarbage('collect')
            for i, product in ipairs(products) do
                assert(product() == expected[i])
                assert(product(nil, false, {}, function() error('ignored') end) == expected[i])
                assert(select('#', product()) == 1)
            end
        end
        return Def.Quad{
            InitCommand = check,
            OnCommand = check,
            UpdateCommand = check,
            EndCommand = check,
        }
    "#;
    let (prepared, _dir) = compile(source, false);
    assert!(prepared.live_scene.is_none());
    let (mut live, _dir) = compile(source, true);
    let mut scene = live.live_scene.take().unwrap();
    assert!(
        scene
            .update(SongLuaLiveFrame::default())
            .unwrap()
            .errors
            .is_empty()
    );
    scene.end().unwrap();
}

#[test]
fn rage_file_buffers_are_available_to_public_song_compilation() {
    let (prepared, dir) = compile(
        r#"
        local file=RageFileUtil.CreateRageFile()
        assert(file:Open('session.ini',2))
        file:PutLine('[Settings]');file:PutLine('Value=7');file:Close()
        local reader=RageFileUtil.CreateRageFile()
        assert(reader:Open('session.ini',1))
        assert(reader:GetLine()=='[Settings]')
        assert(reader:GetLine()=='Value=7')
        assert(not reader:AtEOF())
        assert(reader:GetLine()=='' and reader:AtEOF())
        reader:destroy()
        return Def.Quad{InitCommand=function(self)self:x(7)end}
        "#,
        false,
    );
    assert_eq!(prepared.overlays[0].initial_state.x, 7.);
    assert!(!dir.path().join("session.ini").exists());
}

#[test]
fn authored_warnings_keep_the_public_compilers_side_effect_contract() {
    let (prepared, _dir) = compile(
        r#"
        local before=__songlua_side_effect_count or 0
        assert(select('#',Warn())==0)
        Warn(nil,false,{},function()error('ignored')end)
        assert(__songlua_side_effect_count==before+2)
        assert(select('#',Trace(3,{},false))==0)
        assert(__songlua_side_effect_count==before+2)
        return Def.Quad{InitCommand=function(self)self:x(13)end}
        "#,
        false,
    );
    assert_eq!(prepared.overlays[0].initial_state.x, 13.);
}

#[test]
fn destination_lookup_keeps_raw_conversion_errors_and_stack_recovery() {
    let (mut output, _dir) = compile("return Def.ActorFrame{}", true);
    let mut scene = output.live_scene.take().unwrap();
    fn script(scene: &SongLuaLiveScene, source: &str) {
        scene
            .runtime()
            .vm_mut()
            .execute(&deadlib_lua::compile_named(source.as_bytes(), b"@/navigation.lua").unwrap())
            .unwrap();
    }
    fn top(scene: &SongLuaLiveScene) -> usize {
        scene
            .runtime()
            .vm_mut()
            .with_native_stack(|stack| Ok(stack.top()))
            .unwrap()
    }
    let initial = top(&scene);
    script(&scene, "saved_nav_screen=__songlua_top_screen");
    for (value, expected) in [
        ("nil", None),
        ("''", None),
        ("'ScreenSelectMusic'", Some("ScreenSelectMusic")),
        ("1.25", Some("1.25")),
        ("'A'..string.char(0)..'B'", Some("A\0B")),
    ] {
        script(
            &scene,
            &format!("rawset(__songlua_top_screen,'__songlua_next_screen_name',{value})"),
        );
        assert_eq!(scene.next_screen_name().unwrap().as_deref(), expected);
        assert_eq!(top(&scene), initial);
    }
    for value in ["true", "{}", "function()end", "string.char(255)"] {
        script(
            &scene,
            &format!("rawset(__songlua_top_screen,'__songlua_next_screen_name',{value})"),
        );
        assert!(scene.next_screen_name().is_err(), "{value}");
        assert_eq!(top(&scene), initial);
    }
    script(
        &scene,
        "nav_count=0; __songlua_top_screen=setmetatable({__songlua_next_screen_name='ScreenAfter'},{__index=function()nav_count=nav_count+1;error('getter ran')end})",
    );
    assert_eq!(
        scene.next_screen_name().unwrap().as_deref(),
        Some("ScreenAfter")
    );
    script(
        &scene,
        "rawset(__songlua_top_screen,'__songlua_next_screen_name',nil)",
    );
    assert_eq!(scene.next_screen_name().unwrap(), None);
    assert_eq!(
        scene.runtime().vm_mut().get_global(b"nav_count").unwrap(),
        deadlib_lua::Value::Number(0.)
    );
    script(&scene, "__songlua_top_screen=false");
    assert!(scene.next_screen_name().is_err());
    assert_eq!(top(&scene), initial);
    script(
        &scene,
        "__songlua_top_screen=saved_nav_screen; saved_nav_screen=nil; rawset(__songlua_top_screen,'__songlua_next_screen_name',nil)",
    );
    scene.end().unwrap();
}
