use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, BitmapText, SongRuntime},
};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/bitmap.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
    _files: tempfile::TempDir,
}
impl Pair {
    fn new(live: bool) -> Self {
        Self::with_body(
            live,
            "return Def.ActorFrame{Def.BitmapText{Name='label',Font='Common Normal',Text='initial'},Def.Quad{Name='other'}}",
        )
    }
    fn with_body(live: bool, body: &str) -> Self {
        let files = tempfile::tempdir().unwrap();
        reference_test::register_path(files.path());
        let path = files.path().join("custom.ini");
        std::fs::write(&path, "[main]\nLineSpacing=12\n").unwrap();
        let source = format!(
            r#"
            existing={:?}; directory={:?}; observations={{}}
            function log(...) for i=1,select('#',...) do observations[#observations+1]=tostring(select(i,...)) end end
            {body}
        "#,
            deadsync_song_lua::file_path_string(&path),
            deadsync_song_lua::file_path_string(files.path())
        );
        let mut context = SongLuaCompileContext::new(files.path(), "bitmap migration");
        context.live = live;

        let mut rust = SongRuntime::new(&context).unwrap();
        let def = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(def).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self {
            rust,
            scene,
            _files: files,
        };
        pair.script("label=root[1];other=root[2]", false);
        pair
    }
    fn payload(&mut self) -> BitmapText {
        let value = self.rust.vm_mut().get_global(b"label").unwrap();
        self.rust.actor_bitmap_text(value).unwrap()
    }
    fn script(&mut self, source: &str, read_error: bool) {
        execute(&mut self.rust, source);
        self.compare(source, read_error);
    }
    fn compare(&mut self, phase: &str, read_error: bool) {
        let actor = self.rust.vm_mut().get_global(b"label").unwrap();
        let actual = self.rust.actor_bitmap_text(actor);
        if read_error {
            assert!(actual.is_err(), "{phase}: Rust read");
        } else {
            let actual = actual.unwrap();
            if let Some((_, path)) = &actual.font
                && path.starts_with(self._files.path())
            {
                assert_eq!(path, &self._files.path().join("custom.ini"), "{phase}");
            }
            // Compare the observed physical path above; only its copied debug
            // presentation uses portable separators in the frozen snapshot.
            let mut snapshot = actual.clone();
            if let Some((_, path)) = &mut snapshot.font {
                *path = deadsync_song_lua::file_path_string(path).into();
            }
            reference_test::assert_debug(
                "song/tests/rust_bitmap_text.rs::payload",
                &(phase, &snapshot),
            );
        }
        let actual = self
            .rust
            .actor_scene_snapshot(self.scene)
            .unwrap()
            .into_iter()
            .map(|actor| actor.render)
            .collect::<Vec<_>>();
        reference_test::assert_debug("song/tests/rust_bitmap_text.rs::render", &(phase, &actual));
        let probe = "return table.concat(observations,'|')";
        let Value::String(actual) = execute(&mut self.rust, probe)[0] else {
            panic!("observations")
        };
        reference_test::assert_debug(
            "song/tests/rust_bitmap_text.rs::observations",
            &(phase, &self.rust.vm_mut().string_bytes(actual).unwrap()),
        );
    }
}

#[test]
fn native_fixture_inheritance_and_theme_helpers() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-song-lua-micro/bitmap-methods-input.json"
    ))
    .unwrap();
    let body = fixture["lua_assertions"].as_str().unwrap();
    let (_, assertions) = body.split_once("assert(type(BitmapText.GetX)").unwrap();
    let mut pair = Pair::new(false);
    pair.script(
        &format!("label:settext('');native_label=label;assert(type(BitmapText.GetX){assertions}"),
        false,
    );
    pair.script(r#"
        assert(label:PixelFont()==label);label:Stroke({1,0,.5,.25});label:NoStroke()
        label:DiffuseAndStroke({.5,.4,.3,.2},{.2,.3,.4,.5}):settextf('%04d %s',7,'text')
        assert(label:GetText()=='0007 text')
        local saved=color;color=function(s) log('color',s);return {.4,.3,.2,.1} end;label:NoStroke();color=saved
        local saved=string.format;string.format=function(...) log('format');return 'override' end;label:settextf('anything');string.format=saved
        assert(label:GetText()=='override')
    "#, false);
}

#[test]
fn text_conversion_stroke_errors_and_fresh_color_tables() {
    let mut pair = Pair::new(true);
    for value in [
        "nil",
        "true",
        "false",
        "3",
        "-0.0",
        "1.25",
        "1e20",
        "0/0",
        "1/0",
        "{}",
        "function() end",
        "'a\\0b\\nUTF8:é'",
    ] {
        pair.script(&format!("label:settext({value});log(label:GetText());label.Text={value};log(label:GetText())"),false);
    }
    pair.script(
        "assert(not pcall(label.settext,label,string.char(255)));label.Text=string.char(255)",
        true,
    );
    pair.script("label.Text='valid';label.StrokeColor={.1,.2,.3,.4};local a=label:getstrokecolor();a[1]=9;assert(label:getstrokecolor()[1]~=9)",false);
    for value in [
        "{1,2,3}",
        "{'0x10','2.5',3,.5}",
        "'bad color'",
        "'#11223344'",
        "{}",
        "false",
        "1,2,3,.4",
    ] {
        pair.script(
            &format!("label:strokecolor({value});log(unpack(label:getstrokecolor()))"),
            false,
        );
    }
    pair.script(
        "label.__songlua_stroke_color=false;assert(not pcall(label.getstrokecolor,label))",
        true,
    );
    pair.script("label.__songlua_stroke_color=nil;label.StrokeColor=false;assert(label:getstrokecolor()[1]==0)",false);
}

#[test]
fn attributes_dedup_aliases_raw_reads_and_immutable_publication() {
    let mut pair = Pair::new(true);
    pair.script(r#"
        label:AddAttribute(-2,{Length=-1,Diffuses={{1,0,0},{0,1,0},{0,0,1},{1,1,0}},Glow='#334455'})
        label:AddAttribute(-2,{Length=-1,Diffuses={{1,0,0},{0,1,0},{0,0,1},{1,1,0}},Glow='#334455'})
        assert(#label.__songlua_text_attributes==1)
        label:AddAttribute('2.5',{length='4',diffusecolor={.2,.3,.4}})
        label:AddAttribute(9,{Length=0,Diffuse=setmetatable({},{__index=function() error('not read') end})})
        label:AddAttribute(false,{}):AddAttribute(1,false)
    "#,false);
    let old = pair.payload();
    pair.script("label.__songlua_text_attributes[1].color[1]=.4;label.__songlua_text_attributes[1].vertex_colors[2][2]=.3;label:settext('new')",false);
    assert_eq!(old.text.as_ref(), "initial");
    assert_eq!(old.attributes[0].color[0], 1.);
    for entry in [
        "{start=false,length='2.5',color=false}",
        "{start=-3,length=-1,color={1,2,3,4},vertex_colors={false},glow={1,2,3}}",
        "{start=0,length=0,color=false}",
        "setmetatable({},{__index=function() error('raw only') end})",
    ] {
        pair.script(
            &format!("label.__songlua_text_attributes={{{entry}}}"),
            false,
        );
    }
    pair.script("label.__songlua_text_attributes={false}", true);
    pair.script(
        "label:ClearAttributes();assert(#label.__songlua_text_attributes==0)",
        false,
    );
    pair.script("label.__songlua_text_attributes={{start=0,length=1,color=false}};assert(not pcall(label.AddAttribute,label,0,{}))",false);
    pair.script("label.__songlua_text_attributes={{start=2,length=1,color=false}};label:AddAttribute(0,{});assert(#label.__songlua_text_attributes==2)",false);
}

#[test]
fn collecting_getters_reentry_and_failed_reads_leave_no_stale_payload() {
    let mut pair = Pair::new(true);
    pair.script(r#"
        label:AddAttribute(0,setmetatable({},{__index=function(self,key)
            collectgarbage();other:aux(9);log(key)
            if key=='Length' then return 3 end
            if key=='Diffuses' then return {{.1,.2,.3}} end
            if key=='Glow' then return {1,0,0} end
        end}))
        local mt=getmetatable(label);original=mt.__index
        label.Text=nil;label.Font=nil;label.__songlua_text_attributes=nil
        mt.__index=function(self,key)
            if key=='Text' then collectgarbage();other:x(7);log(key);return 'generated' end
            if key=='Font' then collectgarbage();log(key);return 'Common Normal' end
            if key=='__songlua_text_attributes' then collectgarbage();log(key);return {{start=1,length=-1,color={.4,.5,.6,.7}}} end
            return original(self,key)
        end
    "#,false);
    let saved = pair.payload();

    pair.script("getmetatable(label).__index=function(self,key) if key=='Text' then error('read failure') end return original(self,key) end",true);
    pair.script(
        "getmetatable(label).__index=original;label:settext('recovered')",
        false,
    );
    assert_eq!(saved.text.as_ref(), "generated");
    let value = pair.rust.vm_mut().get_global(b"label").unwrap();
    pair.rust.release_actor_scene(pair.scene).unwrap();
    assert!(pair.rust.actor_bitmap_text(value).is_err());
    assert_eq!(saved.attributes[0].length, usize::MAX);
}

#[test]
fn font_resolution_and_builtin_fallback_match_production() {
    let mut pair = Pair::new(false);
    for value in [
        "nil",
        "''",
        "'Common Normal'",
        "'/Common Normal'",
        "'dir/missing.ini'",
        "'missing.ini'",
        "existing",
        "'custom.ini'",
        "'///@theme:F/missing'",
        "false",
        "42",
    ] {
        pair.script(
            &format!("label.__songlua_script_dir=directory;label.Font={value}"),
            value == "false",
        );
    }
    pair.script(r#"
        label.Font='Common Normal';label.__songlua_script_dir=nil
        local mt=getmetatable(label);local old=mt.__index
        mt.__index=function(self,key) if key=='__songlua_script_dir' then collectgarbage();log('dir');error('path failure') end return old(self,key) end
    "#,false);
}

#[test]
fn legacy_file_font_paths_precede_theme_aliases() {
    let files = tempfile::tempdir().unwrap();
    for name in ["font.ini", "legacy.ini", "42", "43"] {
        std::fs::write(files.path().join(name), "[main]\nLineSpacing=12\n").unwrap();
    }
    for live in [false, true] {
        for (font, file, expected) in [
            ("nil", "legacy", Some("legacy.ini")),
            ("'font.ini'", "legacy", Some("font.ini")),
            ("'font.ini'", "false", Some("font.ini")),
            ("'Common Header'", "legacy", Some("legacy.ini")),
            ("'Common Header'", "false", Some("theme")),
            ("'missing.ini'", "legacy", Some("legacy.ini")),
            ("''", "legacy", Some("legacy.ini")),
            ("999", "legacy", Some("legacy.ini")),
            ("42", "legacy", Some("42")),
            ("nil", "43", Some("43")),
            ("nil", "'Common Header'", Some("theme")),
            ("'Common Header'", "'Common Normal'", Some("theme")),
            ("''", "'Common Header'", None),
        ] {
            let mut context = SongLuaCompileContext::new(files.path(), "legacy File font");
            context.live = live;
            let mut runtime = SongRuntime::new(&context).unwrap();
            let definition = execute(
                &mut runtime,
                &format!(
                    "legacy={:?};return Def.BitmapText{{Font={font},File={file},InitCommand=function(self) self:settext('0') end}}",
                    deadsync_song_lua::file_path_string(&files.path().join("legacy.ini")),
                ),
            )[0];
            let scene = runtime.instantiate_actor(definition).unwrap();
            let actor = runtime.actor_scene_root(scene).unwrap();
            runtime.vm_mut().set_global(b"label", actor).unwrap();
            execute(
                &mut runtime,
                &format!(
                    "label.__songlua_script_dir={:?}",
                    deadsync_song_lua::file_path_string(files.path()),
                ),
            );
            let payload = runtime.actor_bitmap_text(actor).unwrap();
            let expected = expected.map(|name| {
                if name == "theme" {
                    std::path::PathBuf::from("assets/fonts/wendy/_wendy small.ini")
                } else {
                    files.path().join(name)
                }
            });
            assert_eq!(payload.font.map(|(_, path)| path), expected,
                "Font={font}, File={file}, live={live}");
            assert_eq!(payload.text.as_ref(), "0");
        }
    }
}

#[test]
fn common_header_font_uses_native_life_meter_glyphs() {
    use deadlib_assets::FontStore;
    use deadlib_present::font::{find_glyph, measure_line_width_logical, parse};
    use std::path::Path;

    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let native: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join("tests/fixtures/itgmania-fonts/wendy/_wendy small.json")).unwrap(),
    )
    .unwrap();
    let native_one = native["glyphs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|glyph| glyph["codepoint"] == u32::from('1'))
        .unwrap();
    let mut fonts = FontStore::new();
    for live in [false, true] {
        for expression in [
            "'Common header'",
            "'cOmMoN HeAdEr'",
            "THEME:GetPathF('Common', 'Header')",
            "THEME:GetPathF('', 'Common header')",
        ] {
            let files = tempfile::tempdir().unwrap();
            let mut context = SongLuaCompileContext::new(files.path(), "header life meter");
            context.live = live;
            let mut runtime = SongRuntime::new(&context).unwrap();
            let definition = execute(
                &mut runtime,
                &format!("return Def.BitmapText{{Font={expression},Text='111111111'}}"),
            )[0];
            let scene = runtime.instantiate_actor(definition).unwrap();
            let actor = runtime.actor_scene_root(scene).unwrap();
            let payload = runtime.actor_bitmap_text(actor).unwrap();
            let (name, path) = payload.font.unwrap();
            assert_eq!(name, "wendy", "{expression}, live={live}");
            assert_eq!(path, Path::new("assets/fonts/wendy/_wendy small.ini"));

            // Simply Love's Common Header.redir selects this native font.
            // Paperclip draws its life meter with nine '1' glyphs at 50% life.
            fonts.register_font(name, parse(repo.join(path).to_str().unwrap()).unwrap().font);
            let font = fonts.fonts().get(name).unwrap();
            let one = find_glyph(font, '1', fonts.fonts()).unwrap();
            let expected_advance = native_one["advance"].as_i64().unwrap() as i32;
            assert_eq!(one.advance_i32, expected_advance);
            assert_eq!(
                one.size,
                [
                    native_one["width"].as_f64().unwrap() as f32,
                    native_one["height"].as_f64().unwrap() as f32,
                ]
            );
            assert_eq!(
                one.offset,
                [
                    native_one["horizontal_shift"].as_f64().unwrap() as f32,
                    native_one["vertical_shift"].as_f64().unwrap() as f32,
                ]
            );
            assert_eq!(font.height, native["height"].as_i64().unwrap() as i32);
            assert_eq!(
                measure_line_width_logical(font, &payload.text, fonts.fonts()),
                9 * expected_advance
            );
        }
    }
}

#[test]
fn common_header_font_keeps_local_font_precedence() {
    let files = tempfile::tempdir().unwrap();
    let local = files.path().join("Common header.ini");
    std::fs::write(&local, "[main]\nLineSpacing=12\n").unwrap();
    let context = SongLuaCompileContext::new(files.path(), "local font precedence");
    let mut runtime = SongRuntime::new(&context).unwrap();
    let definition = execute(
        &mut runtime,
        "return Def.BitmapText{Font='Common header.ini'}",
    )[0];
    let scene = runtime.instantiate_actor(definition).unwrap();
    let actor = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"label", actor).unwrap();
    execute(
        &mut runtime,
        &format!(
            "label.__songlua_script_dir={:?}",
            deadsync_song_lua::file_path_string(files.path())
        ),
    );
    assert_eq!(
        runtime.actor_bitmap_text(actor).unwrap().font.unwrap().1,
        local
    );
    execute(&mut runtime, "label.Font=THEME:GetPathF('Common','Normal')");
    assert_eq!(
        runtime.actor_bitmap_text(actor).unwrap().font.unwrap().0,
        "miso"
    );
}

#[test]
fn text_layout_strict_flags_fit_bounds_and_zoom_ordering() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        for name in ["jitter", "uppercase", "rainbowscroll"] {
            pair.script(&format!("for _,v in ipairs{{0,1,'true',{{}}}} do assert(not pcall(label.{name},label,v)) end;assert(not pcall(label.{name},label));label:{name}(true)"),false);
        }
        for script in [
            "label:wrapwidthpixels(13.8):vertspacing(-2.5):max_dimension_use_zoom('no'):distort(-1):textglowmode('TextGlowMode_Inner')",
            "label:wrapwidthpixels(-3):vertspacing(2.5):max_dimension_use_zoom():distort(.6):textglowmode('invalid')",
            "label:maxwidth(90):maxheight(80):zoomx(2)",
            "label:zoomy(3):maxwidth(100):zoom(4)",
            "label:zoomto(17,23):zoomtowidth(31):zoomtoheight(37)",
            "label:ClearAttributes():settext(''):SetSize(1,1):zoom(1):zoomtoheight(25)",
            "label:settext('words'):zoomtoheight(19):maxwidth(80):maxheight(60):scaletofit(0,0,200,300)",
            "label:maxwidth(90):maxheight(70):scaletocover(200,300,0,0):undistort()",
        ] {
            pair.script(script, false);
        }
        pair.script(r#"
            label.__songlua_text_saw_max_width=nil
            local mt=getmetatable(label);local old=mt.__index
            mt.__index=function(self,key) if key=='__songlua_text_saw_max_width' then error('zoom flag') end return old(self,key) end
            assert(not pcall(label.zoom,label,7));assert(label:GetZoomX()==7)
            mt.__index=old
        "#,false);
    }
}

#[test]
fn load_font_templates_concat_composition_and_independent_attributes() {
    for live in [false, true] {
        let mut pair = Pair::with_body(
            live,
            r#"
            font=LoadFont('Common Normal'):settext('template'):x(6):maxwidth(80):zoom(2)
            assert(font.Class==nil and font.Font=='Common Normal' and font:GetText()=='template')
            font:AddAttribute(0,{Length=2,Diffuse={.1,.2,.3,.4}}):strokecolor({.4,.3,.2,.1})
            font.InitCommand=function(self) log('first',self:GetText());self:x(10) end
            font.CustomCommand=function(self,...) log('custom first',...);return 'unused' end
            font.Condition=false
            merged=font..{
                Name='label',Text='instance',Condition=true,
                __songlua_actor_type='Quad',__songlua_script_dir='ignored',
                InitCommand=function(self) log('second');self:addx(3) end,
                CustomCommand=function(self,...) log('custom second',...);return 4,nil,7 end,
            }
            assert(merged~=font and merged:GetX()==6 and #observations==0)
            assert(merged.__songlua_actor_type=='BitmapText' and merged.__songlua_script_dir~='ignored')
            merged.__songlua_text_attributes[1].color[1]=.9
            assert(font.__songlua_text_attributes[1].color[1]~=.9)
            local a,b,c=merged:CustomCommand('param',nil,2);assert(a==4 and b==nil and c==7)
            return Def.ActorFrame{merged,Def.Quad{Name='other'}}
        "#,
        );
        // The frozen preparation capture retains x=6, so native addx uses 6+3.
        // A live template already owns replay and uses the preceding x(10).
        pair.script(&format!("assert(label:GetText()=='instance' and label:GetX()=={});assert(font:GetText()=='template' and font:GetX()==6);log(label:GetZoomX())", if live {13} else {9}), false);
        for font in [
            "nil",
            "false",
            "42",
            "{}",
            "string.char(255)",
            "'Common Normal'",
        ] {
            pair.script(&format!("local f=LoadFont({font});log(f.Font,f:GetText(),f:GetX());assert(not pcall(function() return f..3 end))"),false);
        }
        pair.script("local saved=font.__songlua_text_attributes[1].color[1];label.__songlua_text_attributes[1].color[1]=.8;assert(font.__songlua_text_attributes[1].color[1]==saved);collectgarbage()",false);
    }
}

#[test]
fn retained_text_updates_during_live_tweens_and_draw_callbacks() {
    let mut pair = Pair::new(true);

    pair.rust.begin_actor_playback();
    pair.script(r#"
        label:linear(1):x(100):maxwidth(80):zoom(2):jitter(true):settext('queued pose')
        root:SetUpdateFunction(function(self,delta)
            collectgarbage();label:settextf('%.2f',delta):ClearAttributes():AddAttribute(0,{Length=-1,Diffuse={.2,.4,.6,.8}})
        end)
        root:SetDrawFunction(function(self)
            label:settext(label:GetText()..' drawn');label:Draw()
        end)
    "#,false);
    for (seconds, delta) in [(0.25, 0.25), (0.75, 0.5), (1.25, 0.5)] {
        pair.rust.advance_actor_scene(pair.scene, delta).unwrap();
        pair.compare("live update", false);

        pair.rust.draw_actor_scene(pair.scene).unwrap();
        pair.compare("draw callback", false);
        let actor = pair.rust.vm_mut().get_global(b"label").unwrap();
        let root = pair.rust.actor_scene_root(pair.scene).unwrap();
        let actual = pair
            .rust
            .take_actor_draw_frames(&[
                (actor, deadsync_song_lua::SongLuaDrawSource::Overlay(0)),
                (root, deadsync_song_lua::SongLuaDrawSource::Overlay(1)),
            ])
            .unwrap();
        reference_test::assert_debug(
            "song/tests/rust_bitmap_text.rs::draw_frames",
            &(seconds, delta, &actual),
        );
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
