use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime},
};

fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/text-displays.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
    _files: tempfile::TempDir,
}
impl Pair {
    fn new(live: bool) -> Self {
        let files = tempfile::tempdir().unwrap();
        reference_test::register_path(files.path());
        let mut context = SongLuaCompileContext::new(files.path(), "text display migration");
        context.live = live;
        let source = r#"
            observations={}
            function log(...) for i=1,select('#',...) do observations[#observations+1]=tostring(select(i,...)) end end
            return Def.ActorFrame{
                Def.RollingNumbers{Name='rolling',Font='Common Normal',Text='start'},
                Def.BPMDisplay{Name='bpm',Font='Common Normal'},
                Def.DeviceList{Name='devices',Font='Common Normal',Text='overwritten'},
                Def.InputList{Name='inputs',Font='Common Normal',Text='overwritten'},
            }
        "#;

        let mut rust = RustSongRuntime::new(&context).unwrap();
        let definition = execute(&mut rust, source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self {
            rust,
            scene,
            _files: files,
        };
        pair.script(
            "rolling=root[1];bpm=root[2];devices=root[3];inputs=root[4]",
            None,
        );
        pair
    }
    fn script(&mut self, source: &str, error: Option<&str>) {
        execute(&mut self.rust, source);
        self.compare(source, error);
    }
    fn compare(&mut self, phase: &str, error: Option<&str>) {
        for name in ["rolling", "bpm", "devices", "inputs"] {
            let actor = self.rust.vm_mut().get_global(name.as_bytes()).unwrap();
            let actual = self.rust.actor_bitmap_text(actor);
            if error == Some(name) {
                assert!(actual.is_err(), "{phase}: Rust {name}");
            } else {
                reference_test::assert_debug(
                    "song/tests/rust_text_displays.rs::payload",
                    &(phase, name, &actual.unwrap()),
                );
            }
        }
        let actual = self
            .rust
            .actor_scene_snapshot(self.scene)
            .unwrap()
            .into_iter()
            .map(|actor| actor.render)
            .collect::<Vec<_>>();
        reference_test::assert_debug(
            "song/tests/rust_text_displays.rs::render",
            &(phase, &actual),
        );
        let probe = "return table.concat(observations,'|')";
        let Value::String(actual) = execute(&mut self.rust, probe)[0] else {
            panic!("observations")
        };
        reference_test::assert_debug(
            "song/tests/rust_text_displays.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
}

#[test]
fn construction_lookup_and_existing_rolling_formats() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script(r#"
            assert(RollingNumbers==nil and BPMDisplay==nil and DeviceList==nil and InputList==nil)
            assert(rolling:GetText()=='start' and bpm:GetText()=='')
            assert(devices:GetText()=='No input devices' and inputs:GetText()=='No unmapped inputs')
            for _,a in ipairs{rolling,bpm,devices,inputs} do
                assert(a.GetText~=nil and a.SetUpdateFunction==nil and a.SetDrawFunction==nil and a.GetDrawFunction==nil)
                assert(a.PixelFont==nil and a.NoStroke==nil and a.UnknownMethod==nil)
                assert(a:GetTargetNumber()==0)
                a:settext('text'):AddAttribute(0,{Length=-1,Diffuse={.1,.2,.3,.4}}):strokecolor({.4,.3,.2,.1})
            end
            assert(rolling:Load('RollingNumbersEvaluationA')==rolling)
            assert(rolling:targetnumber(12)==rolling and rolling:GetText()=='0012' and rolling:GetTargetNumber()==12)
            rolling:Load('RollingNumbersEvaluationB');assert(rolling:GetText()=='0012')
            rolling:SetTargetNumber(7);assert(rolling:GetText()=='007')
        "#,None);
        for metric in [
            "RollingNumbersEvaluation",
            "rollingnumbersevaluationnodecentswayoffs",
            "unknown",
            "",
            "RollingNumbersEvaluationB",
        ] {
            pair.script(&format!("rolling:Load({metric:?}):targetnumber(-12.5);log(rolling:GetText(),rolling:GetTargetNumber())"),None);
        }
        pair.script("local old=rolling.__songlua_rolling_numbers_format;rolling:Load(false):Load(19):Load({}):Load();assert(rolling.__songlua_rolling_numbers_format==old)",None);
    }
}

#[test]
fn target_number_coercions_and_format_failure_publication() {
    let mut pair = Pair::new(true);
    for format in [
        "nil",
        "'%.0f'",
        "'%03.0f'",
        "'%04.0f'",
        "'%.2f'",
        "'%04 %.2'",
        "17",
    ] {
        for number in [
            "0", "-0.0", "1.5", "-1.5", "16777217", "1e30", "'2.75'", "'NaN'", "'inf'", "1/0",
            "0/0", "'0x10'", "false", "{}", "nil",
        ] {
            pair.script(&format!("rolling.__songlua_rolling_numbers_format={format};rolling:targetnumber({number});log(rolling:GetText(),rolling:GetTargetNumber())"),None);
        }
    }
    pair.script(r#"
        rolling.__songlua_rolling_numbers_format=false;local old=rolling:GetText()
        assert(not pcall(rolling.targetnumber,rolling,23));assert(rolling:GetTargetNumber()==23 and rolling:GetText()==old)
        rolling.__songlua_rolling_numbers_format=nil
        local mt=getmetatable(rolling);local oldindex=mt.__index
        mt.__index=function(self,key)
            if key=='__songlua_rolling_numbers_format' then collectgarbage();bpm:x(12);log('format read');return '%03.0f' end
            return oldindex(self,key)
        end
        rolling:targetnumber(3);assert(rolling:GetText()=='003');mt.__index=oldindex
        rolling.__songlua_target_number='0x20';assert(rolling:GetTargetNumber()==32)
        rolling.__songlua_target_number=false;assert(not pcall(rolling.GetTargetNumber,rolling))
        rolling.__songlua_target_number=nil;assert(rolling:GetTargetNumber()==0)
    "#,None);
}

#[test]
fn bound_receivers_dot_calls_and_dynamic_formatting() {
    let mut pair = Pair::new(true);
    pair.script(r#"
        local set=rolling.targetnumber;set(4);assert(rolling:GetTargetNumber()==4)
        set(bpm,9);assert(rolling:GetTargetNumber()==9 and bpm:GetTargetNumber()==0)
        local text=rolling.settext;text('ignored');assert(rolling:GetText()=='')
        text(bpm,'owned');assert(rolling:GetText()=='owned' and bpm:GetText()=='')
        local fmt=rolling.settextf;fmt('%03d',5);assert(rolling:GetText()=='005')
        rolling:settextf();assert(rolling:GetText()=='')
        local oldset=rolling.settext;rolling.settext=function() error('must not dispatch settext') end
        rolling:settextf('%s','direct');rolling.settext=oldset;assert(rolling:GetText()=='direct')
        local original=string.format
        string.format=function(...) collectgarbage();bpm:targetnumber(8);log('formatter',...);return true end
        rolling:settextf('unused',nil,3);assert(rolling:GetText()=='true')
        string.format=setmetatable({},{__call=function() return 'callable' end})
        assert(not pcall(rolling.settextf,rolling,'unused'));rolling:settextf();assert(rolling:GetText()=='')
        string.format=original
        local inherited=Actor.GetX;Actor.GetX=function() return 71 end;assert(rolling:GetX()==71);Actor.GetX=inherited
        local bitmap=BitmapText.GetText;BitmapText.GetText=function() error('wrong class') end
        assert(rolling:GetText()=='');BitmapText.GetText=bitmap
        local own=Actor.GetText;Actor.GetText=function() return 'public actor' end;assert(rolling:GetText()=='public actor');Actor.GetText=own
        local bound=rolling.__songlua_bound_methods;rolling.__songlua_bound_methods=false
        assert(not pcall(function() return rolling.GetText end));rolling.__songlua_bound_methods=bound
    "#,None);
}

#[test]
fn text_layout_for_unlinked_displays_uses_generic_fit_geometry() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        for source in [
            "for _,a in ipairs{rolling,bpm,devices,inputs} do a:settext('words'):maxwidth(90):maxheight(80):zoom(2):wrapwidthpixels(35):vertspacing(-3):uppercase(true):jitter(true):rainbowscroll(true) end",
            "for _,a in ipairs{rolling,bpm,devices,inputs} do a:zoomto(17,19):zoomtowidth(31):zoomtoheight(37):mult_attrs_with_diffuse(0);assert(a:get_mult_attrs_with_diffuse()) end",
            "for _,a in ipairs{rolling,bpm,devices,inputs} do a:scaletofit(0,0,80,100):maxwidth(40):zoomx(3):maxheight(50):zoomy(4) end",
            "for _,a in ipairs{rolling,bpm,devices,inputs} do a:ClearAttributes():undistort():textglowmode('TextGlowMode_Inner') end",
        ] {
            pair.script(source, None);
        }
    }
}

#[test]
fn status_resources_keep_fixed_labels_and_ignore_authored_style_payloads() {
    let mut pair = Pair::new(true);
    pair.script(
        r#"
        devices:settext('authored devices');inputs:settext('authored inputs')
        assert(devices:GetText()=='authored devices' and inputs:GetText()=='authored inputs')
        devices.__songlua_stroke_color=false;inputs.__songlua_text_attributes=false
        devices.Text=string.char(255);inputs.Text=string.char(255)
    "#,
        None,
    );
    pair.script("devices.Font=false", Some("devices"));
    pair.script(
        "devices.Font='Common Normal';devices.Text='recovered';inputs.Text='recovered'",
        None,
    );
    let actor = pair.rust.vm_mut().get_global(b"devices").unwrap();
    let retained = pair.rust.actor_bitmap_text(actor).unwrap();
    assert_eq!(retained.text.as_ref(), "No input devices");
    assert!(retained.attributes.is_empty());
    assert!(retained.stroke_color.is_none());
    pair.rust.release_actor_scene(pair.scene).unwrap();
    assert!(pair.rust.actor_bitmap_text(actor).is_err());
    assert_eq!(retained.text.as_ref(), "No input devices");
}

#[test]
fn live_numeric_callbacks_gc_and_template_copy_keep_owner_identity() {
    let mut pair = Pair::new(true);

    pair.rust.begin_actor_playback();
    pair.script(r#"
        rolling:Load('RollingNumbersEvaluationA'):targetnumber(2):AddAttribute(0,{Diffuse={.1,.2,.3,.4}})
        clone=rolling..{Name='clone'};clone:targetnumber(7)
        clone.__songlua_text_attributes[1].color[1]=.9
        assert(rolling:GetText()=='0002' and clone:GetText()=='0007')
        assert(rolling.__songlua_text_attributes[1].color[1]~=.9)
        saved=rolling.targetnumber
        root:SetUpdateFunction(function(self,delta) collectgarbage();saved(rolling:GetTargetNumber()+delta);bpm:settextf('%.2f',delta) end)
    "#,None);
    for (seconds, delta) in [(0.25, 0.25), (0.75, 0.5)] {
        pair.rust.advance_actor_scene(pair.scene, delta).unwrap();
        pair.compare("live numeric update", None);
    }
    pair.rust.release_actor_scene(pair.scene).unwrap();
    execute(
        &mut pair.rust,
        "assert(not pcall(saved,9));rolling=nil;bpm=nil;devices=nil;inputs=nil;root=nil;saved=nil;collectgarbage()",
    );
}

#[test]
fn load_keeps_rolling_metrics_separate_from_other_display_textures() {
    let mut pair = Pair::new(true);
    let file = pair._files.path().join("display.png");
    image::RgbaImage::from_pixel(12, 8, image::Rgba([255; 4]))
        .save(&file)
        .unwrap();
    let path = deadsync_song_lua::file_path_string(&file);
    pair.script(
        &format!(
            r#"
        path={path:?}
        assert(bpm:Load(path)==bpm);assert(bpm:GetWidth()==12 and bpm:GetHeight()==8)
        assert(devices:Load(path)==devices);assert(inputs:Load(path)==inputs)
        assert(rolling:Load(path)==rolling);assert(rolling.Texture==nil)
        rolling:targetnumber(12);assert(rolling:GetText()=='12')
        local load=bpm.Load;load(inputs,nil);assert(bpm.Texture==nil and inputs.Texture==path)
        bpm:Load(path);bpm.__songlua_actor_type='RollingNumbers'
        bpm:Load('RollingNumbersEvaluationA'):targetnumber(3)
        assert(bpm.Texture==path and bpm:GetText()=='0003')
        bpm.__songlua_actor_type='BPMDisplay'
        devices:Load(nil);inputs:Load(nil)
    "#
        ),
        None,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
