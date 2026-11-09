use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaNoteskinResolver, rust_engine::RustSongRuntime,
};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
};

thread_local! { static ROOT: RefCell<PathBuf> = RefCell::new(PathBuf::new()); }
fn resolver() -> SongLuaNoteskinResolver {
    SongLuaNoteskinResolver {
        resolve_path: |skin, _, element| {
            ROOT.with(|root| {
                let path = root
                    .borrow()
                    .join(skin.trim().to_ascii_lowercase())
                    .join(element);
                (path.is_file() || element == "missing.lua").then_some(path)
            })
        },
        metric: |skin, element, key| {
            Some(match key {
                "Echo" => format!("{skin}/{element}"),
                "Fraction" => "2.5".into(),
                "Negative" => "-2.5".into(),
                "Huge" => "1e100".into(),
                "NaN" => "NaN".into(),
                "Bad" => "text".into(),
                "Command" => "x,17;y,-2".into(),
                "Expression" => "%function(self) self:x(23); return 'command' end".into(),
                "InvalidFunction" => "%7".into(),
                "Syntax" => "%function(".into(),
                _ => return None,
            })
        },
        metric_f: |_, _, key| match key {
            "Fraction" => Some(1.1),
            "NaN" => Some(f32::NAN),
            "Huge" => Some(f32::INFINITY),
            _ => None,
        },
        metric_b: |_, _, key| match key {
            "True" => Some(true),
            "False" => Some(false),
            _ => None,
        },
        exists: |name| {
            ["cel", "metal", "my skin"]
                .iter()
                .any(|n| n.eq_ignore_ascii_case(name))
        },
        names: || vec!["cel".into(), "Metal".into(), "my skin".into()],
    }
}
fn context() -> SongLuaCompileContext {
    let mut c = SongLuaCompileContext::new(Path::new("."), "noteskin manager migration");
    c.live = true;
    c.players[0].noteskin_name = "cel".into();
    c
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/noteskin.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}
fn number(n: f64) -> Scalar {
    Scalar::Number(if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    })
}
struct Pair {
    dir: tempfile::TempDir,
    rust: RustSongRuntime,
}
impl Pair {
    fn new(mut c: SongLuaCompileContext) -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        for path in ["cel", "metal", "Local Skin", "Not A Skin"] {
            fs::create_dir(dir.path().join(path)).unwrap();
        }
        fs::write(dir.path().join("Local Skin/metrics.ini"), "[Global]\n").unwrap();
        ROOT.with(|root| *root.borrow_mut() = dir.path().to_path_buf());
        c.song_dir = dir.path().to_path_buf();

        let rust = RustSongRuntime::new_with_noteskin_resolver(&c, resolver()).unwrap();
        Self { dir, rust }
    }
    fn write(&self, path: &str, contents: &str) {
        fs::write(self.dir.path().join(path), contents).unwrap();
    }
    fn compare(&mut self, source: &str) {
        let actual: Vec<_> = execute(&mut self.rust, source)
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(b) => Scalar::Boolean(b),
                Value::Number(n) => number(n),
                Value::String(id) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
                }
                v => panic!("scalar expected {v:?}"),
            })
            .collect();
        reference_test::assert_debug("song/tests/rust_noteskin.rs::observations", &actual);
    }
}

#[test]
fn metrics_surface_defaults_conversion_and_receiver_rules_match_production() {
    for enabled in [[true, false], [false, true], [false, false]] {
        let mut c = context();
        for (p, enabled) in c.players.iter_mut().zip(enabled) {
            p.enabled = enabled;
        }
        c.players[1].noteskin_name = "Metal".into();
        let mut p = Pair::new(c);
        p.compare("local names={};for k in pairs(NOTESKIN) do names[#names+1]=k end;table.sort(names);return table.concat(names,',')");
        for method in [
            "GetMetric",
            "GetMetricF",
            "GetMetricI",
            "GetMetricB",
            "GetPath",
        ] {
            for key in [
                "Echo", "Fraction", "Negative", "Huge", "NaN", "Bad", "Missing", "True", "False",
            ] {
                p.compare(&format!("return NOTESKIN:{method}('Element','{key}'),NOTESKIN:{method}ForNoteSkin('Element','{key}',' custom ')"));
            }
            for args in [
                "nil,'a','b'",
                "false,'a','b'",
                "{},17,23",
                "{},true,'b'",
                "{},'a'",
                "{},string.char(255),'b'",
                "{},'a','b','ignored'",
            ] {
                p.compare(&format!(
                    "local ok=pcall(NOTESKIN.{method},{args});return ok"
                ));
            }
        }
        p.compare("local ok=pcall(NOTESKIN.GetMetricForNoteSkin,{},'a','b');return ok");
    }
}

#[test]
fn discovery_snapshots_local_skins_and_returns_fresh_resolver_lists() {
    let mut p = Pair::new(context());
    for name in [
        "cel",
        "METAL",
        "local skin",
        "Local Skin ",
        "Not A Skin",
        "later",
        "unknown",
    ] {
        p.compare(&format!("return NOTESKIN:DoesNoteSkinExist('{name}')"));
    }
    // Local membership is frozen at construction; names stay resolver-owned.
    fs::remove_file(p.dir.path().join("Local Skin/metrics.ini")).unwrap();
    fs::create_dir(p.dir.path().join("later")).unwrap();
    p.write("later/metrics.ini", "");
    p.compare(r#"
        assert(NOTESKIN:DoesNoteSkinExist('LOCAL SKIN') and not NOTESKIN:DoesNoteSkinExist('later'))
        local a=NOTESKIN:GetNoteSkinNames();local b=NOTESKIN.GetNoteSkinNames(false)
        assert(a~=b);a[1]='changed'
        local v=NOTESKIN:GetVariantNamesForNoteSkin();assert(v~=NOTESKIN.GetVariantNamesForNoteSkin(nil))
        return table.concat(b,','),#v,NOTESKIN:HasVariants(),NOTESKIN.IsNoteSkinVariant('anything')
    "#);
}

#[test]
fn metric_commands_use_original_compiler_and_current_global_environment() {
    let mut p = Pair::new(context());
    p.compare(r#"
        local a={x=function(self,n) self.px=n;return self end,y=function(self,n)self.py=n;return self end}
        loadstring=function()error('replaced compiler')end
        local first=NOTESKIN:GetMetricA('Any','Command');first(a)
        local x,y=a.px,a.py
        local second=NOTESKIN:GetMetricAForNoteSkin('Any','Expression','metal')
        collectgarbage('collect');local result=second(a)
        local empty=NOTESKIN:GetMetricA('Any','Missing');empty(a)
        return x,y,a.px,result,(pcall(NOTESKIN.GetMetricA,NOTESKIN,'Any','InvalidFunction')),
            (pcall(NOTESKIN.GetMetricA,NOTESKIN,'Any','Syntax'))
    "#);
}

#[test]
fn nested_lua_templates_keep_skin_button_and_fallback_after_gc_and_global_replacement() {
    let mut p = Pair::new(context());
    p.write("cel/outer.lua", r#"
        local child=NOTESKIN:LoadActor('Right','inner.lua')
        local sibling=NOTESKIN:LoadActorForNoteSkin('Down','inner.lua','metal')
        local relative=assert(loadfile('inner.lua'))()
        return Def.ActorFrame{ child,sibling,
            Probe=function()return Var('Button')..'/'..Var('Element')..'/'..NOTESKIN:GetMetric('E','Echo')..'/'..Var('Extra') end,
            Child=child.Probe,Sibling=sibling.Probe,Relative=relative.Probe,
            Local=function()return NOTESKIN:DoesNoteSkinExist('Local Skin') end,
        }
    "#);
    for path in ["cel/inner.lua", "metal/inner.lua"] {
        p.write(path, "return Def.Actor{Probe=function() collectgarbage('collect');return Var('Button')..'/'..Var('Element')..'/'..NOTESKIN:GetMetric('E','Echo')..'/'..Var('Extra') end}");
    }
    p.compare(
        r#"
        local held={value='fallback'}
        Var=function(name)collectgarbage('collect');return held.value..name end
        local top=NOTESKIN
        a=top:LoadActor('Left','outer.lua');b=top:LoadActorForNoteSkin('Up','inner.lua','metal')
        assert(a.__songlua_noteskin_name=='cel' and a.__songlua_noteskin_button=='Left')
        assert(top:DoesNoteSkinExist('Local Skin') and not a.Local())
        Var=function()return 'new fallback'end;NOTESKIN={};collectgarbage('collect')
        return a.Probe(),a.Child(),a.Sibling(),a.Relative(),b.Probe(),a.__songlua_noteskin_element
    "#,
    );
}

#[test]
fn exact_file_errors_restore_context_and_preserve_tagging_metamethods() {
    let mut p = Pair::new(context());
    p.write("cel/error.lua", "error('template failure')");
    p.write("cel/value.lua", "return 17");
    p.write("cel/syntax.lua", "function(");
    p.write(
        "cel/tag.lua",
        r#"
        return setmetatable({}, {__newindex=function(t,k,v)
            collectgarbage('collect');_G.tags=(_G.tags or '')..k..'='..v..';';rawset(t,k,v)
        end})
    "#,
    );
    p.compare(r#"
        __songlua_script_dir='before';__songlua_current_script_path='path';__songlua_current_chunk_env={old=true}
        local env=__songlua_current_chunk_env
        for _,f in ipairs{'error.lua','value.lua','syntax.lua','missing.lua'}do
            assert(not pcall(NOTESKIN.LoadActor,NOTESKIN,'Left',f))
            assert(__songlua_script_dir=='before' and __songlua_current_script_path=='path' and __songlua_current_chunk_env==env)
        end
        local a=NOTESKIN:LoadActorForNoteSkin('Left','tag.lua',' CEL ')
        return tags,a.__songlua_noteskin_name,Var('Button')
    "#);
}

#[test]
fn image_and_missing_templates_support_methods_and_independent_concatenation() {
    let mut p = Pair::new(context());
    // The native factory only assigns Texture here; resource decoding is a later phase.
    p.write("cel/image.png", "");
    p.write("cel/other.ogg", "");
    p.compare(r#"
        local a=NOTESKIN:LoadActor('Left','image.png');a:x(7)
        local b=a..{Name='copy',Probe=function(self)self:y(9)end};b:x(11);b.Probe(b)
        local missing=NOTESKIN:LoadActor('Down','missing');missing:x(5)
        local audio=NOTESKIN:LoadActor('Up','other.ogg')
        return a.__songlua_actor_type,a:GetX(),b:GetX(),b:GetY(),a:GetY(),a.Texture==b.Texture,
            b.__songlua_noteskin_name,missing.__songlua_actor_type,missing:GetX(),audio.__songlua_actor_type,a.Class
    "#);
}

#[test]
fn retained_lookup_does_not_retain_manager_and_actual_player_options_uses_it() {
    let mut p = Pair::new(context());
    p.compare(
        r#"
        local o=GAMESTATE:GetPlayerState('PlayerNumber_P1'):GetPlayerOptions('ModsLevel_Stage')
        o:NoteSkin('METAL');local a=o:NoteSkin()
        GAMESTATE:ApplyStageModifiers('PlayerNumber_P1','local skin');local b=o:NoteSkin()
        assert(GAMESTATE:PlayerIsUsingModifier('PlayerNumber_P1','local skin'))
        local weak=setmetatable({NOTESKIN},{__mode='v'});keep=NOTESKIN.GetMetric
        NOTESKIN=nil;collectgarbage('collect');collectgarbage('collect')
        assert(weak[1]==nil)
        return a,b,keep({},'E','Echo')
    "#,
    );
}

#[test]
fn var_game_command_preserves_dynamic_queries_errors_and_fresh_identity() {
    let mut p = Pair::new(context());
    p.compare(r#"
        assert(Var('GameCommand')~=Var('GameCommand'))
        local c=Var('GameCommand');local keys={};for k in pairs(c)do keys[#keys+1]=k end;table.sort(keys)
        local song={GetGroupName=function()collectgarbage('collect');return 17 end}
        GAMESTATE={GetCurrentSong=function(self)return song end,GetCurrentStyle=function()return 'style'end,
            GetCurrentSteps=function(self,pn)return pn end,GetCurrentTrail=function(self,pn)return pn..'Trail'end}
        local text=table.concat(keys,',');c:ApplyToStyle();c.ApplyToStyle(nil)
        local g=c:GetSongGroup();GAMESTATE.GetCurrentSong=false
        local bad=pcall(c.GetSong,c);GAMESTATE=nil
        return text,g,c:GetSong(),c:GetSongGroup(),c:GetIndex(),c:GetMultiPlayer(),c:GetDifficulty(),bad,
            Var('Other'),Var(2),Var(string.char(255)),__songlua_side_effect_count
    "#);
    p.compare(
        r#"
        local c=Var('GameCommand');GAMESTATE={GetCurrentSteps=function(self,pn)return pn end}
        return c:GetSteps(),c:GetTrail(),c:GetStyle(),c:GetText(),c:GetUrl()
    "#,
    );
}

#[test]
fn delayed_template_commands_survive_collection_and_run_through_scene_teardown() {
    for live in [false, true] {
        let mut c = context();
        c.live = live;
        let mut p = Pair::new(c.clone());
        p.write(
            "cel/delayed.lua",
            r#"
            return Def.Actor{
                InitCommand=function(self)self:x(5);_G.initial=Var('Button')end,
                OnCommand=function(self)self:sleep(.25):queuecommand('Later')end,
                LaterCommand=function(self)
                    collectgarbage('collect');self:x(19)
                    _G.later=Var('Button')..'/'..Var('Element')..'/'..NOTESKIN:GetMetric('E','Echo')
                end,
                EndCommand=function(self)_G.ended=Var('Button')end,
            }
        "#,
        );
        c.song_dir = p.dir.path().to_path_buf();
        let source = "return NOTESKIN:LoadActor('Left','delayed.lua')";

        let declaration = execute(&mut p.rust, source)[0];
        let scene = p.rust.instantiate_actor(declaration).unwrap();
        let root = p.rust.actor_scene_root(scene).unwrap();
        p.rust.vm_mut().set_global(b"root", root).unwrap();
        p.compare(
            "NOTESKIN=nil;Var=nil;collectgarbage('collect');return root:GetX(),initial,later",
        );

        p.rust.start_actor_scene(scene).unwrap();

        p.rust.advance_actor_scene(scene, 0.5).unwrap();
        p.compare("return root:GetX(),initial,later");

        p.rust.end_actor_scene(scene).unwrap();
        p.compare("return ended");
        assert!(p.rust.take_actor_diagnostics().is_empty());
        assert!(p.rust.release_actor_scene(scene).unwrap());
        assert!(p.rust.actor_scene_snapshot(scene).is_none());
        execute(
            &mut p.rust,
            "local saved=root;root=nil;collectgarbage('collect');assert(not pcall(saved.x,saved,1))",
        );
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
