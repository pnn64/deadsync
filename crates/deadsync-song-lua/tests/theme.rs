use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{self as song, SongLuaCompileContext, SongRuntime};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    SongLuaCompileContext::new(Path::new("."), "theme manager migration")
}
fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/theme.lua").unwrap())
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
    rust: SongRuntime,
}
impl Pair {
    fn new(context: &SongLuaCompileContext) -> Self {
        Self {
            rust: SongRuntime::new(context).unwrap(),
        }
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
                other => panic!("scalar expected: {other:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_theme.rs::Pair::compare",
            source,
            &actual,
        );
    }
}

#[test]
fn theme_identity_methods_fresh_lists_and_side_effects_match_production() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local t=THEME
        assert(select('#',t:RunLuaScripts('ignored'))==0 and select('#',t:ReloadMetrics())==0 and select('#',t:SetTheme('unknown'))==0)
        local names=t:GetSelectableThemeNames();local fallback=t:get_theme_fallback_list()
        assert(names~=t:GetSelectableThemeNames() and names~=fallback and #names==1 and #fallback==1)
        names[1]='changed';fallback[2]='extra';collectgarbage('collect')
        return t:GetCurThemeName(),t:GetThemeDisplayName(),t:GetCurLanguage(),t:GetCurrentThemeDirectory(),t:GetThemeAuthor(),
            t:GetNumSelectableThemes(),table.concat(t:GetSelectableThemeNames(),','),table.concat(t:get_theme_fallback_list(),','),__songlua_side_effect_count
    "#);
    for value in [
        "'Simply Love'",
        "'simply love'",
        "'en'",
        "'ENGLISH'",
        "'no'",
        "''",
        "false",
        "nil",
        "12",
        "{}",
        "string.char(255)",
    ] {
        p.compare(&format!("local v={value};return THEME:DoesThemeExist(v),THEME.IsThemeSelectable({{}},v),THEME.DoesLanguageExist(v)"));
    }
    p.compare("__songlua_side_effect_count={};local ok=pcall(THEME.SetTheme,'name');__songlua_side_effect_count=0;return ok,THEME:GetCurThemeName()");
}

#[test]
fn theme_paths_preserve_argument_rules_aliases_and_path_info_results() {
    let mut p = Pair::new(&context());
    for method in [
        "GetPathB",
        "GetPathF",
        "GetPathG",
        "GetPathO",
        "GetPathS",
        "GetPathInfoB",
    ] {
        for args in [
            "'','Common Normal'",
            "'/ScreenGameplay/','/in.png'",
            "'', 'ScreenGameplay in'",
            "'', '_VisualStyles/Spooky/ExtraSpooky/Bats'",
            "nil,17",
            "false,{}",
            "' group ', 'a/b'",
            "'a\\\\b','c'",
            "string.char(255),'x'",
        ] {
            p.compare(&format!("return THEME:{method}({args})"));
        }
        p.compare(&format!("local f=THEME.{method};return f('A','B')"));
    }
}

#[test]
fn metric_definitions_enumeration_and_context_numbers_match_production() {
    for (width, height, second) in [
        (640., 480., false),
        (854.25, 720.25, true),
        (319.5, 240., false),
    ] {
        let mut context = context();
        context.screen_width = width;
        context.screen_height = height;
        context.players[0].enabled = true;
        context.players[1].enabled = second;
        let mut p = Pair::new(&context);
        p.compare("SL={Global={GameMode='ITG',ActiveModifiers={MusicRate=1.25}},Metrics={ITG={}}}");
        for group in [
            "Common",
            "ScreenGameplay",
            "Player",
            "Combo",
            "GraphDisplay",
            "LifeMeterBar",
            "ScoreKeeperNormal",
            "MusicWheel",
            "PlayerStageStats",
            "ScreenHeartEntry",
            "ScreenPlayerOptions",
            "ScreenPlayerOptions2",
            "ScreenPlayerOptions3",
            "ScreenAttackMenu",
            "ScreenOptionsService",
            "ScreenSystemOptions",
            "ScreenInputOptions",
            "ScreenGraphicsSoundOptions",
            "ScreenVisualOptions",
            "ScreenAppearanceOptions",
            "ScreenArcadeOptions",
            "ScreenAdvancedOptions",
            "ScreenThemeOptions",
            "ScreenMenuTimerOptions",
            "ScreenUSBProfileOptions",
            "ScreenTournamentModeOptions",
            "ScreenGrooveStatsOptions",
            "Unknown",
        ] {
            p.compare(&format!(
                "local n=THEME:GetMetricNamesInGroup({group:?});return n and table.concat(n,'|')"
            ));
            let mut names = song::theme_metric_names(group);
            names.extend(["Class", "NoSuchMetric"].map(str::to_string));
            for name in names {
                p.compare(&format!("return THEME:GetMetric({group:?},{name:?}),THEME:GetMetricF({group:?},{name:?}),THEME:GetMetricI({group:?},{name:?}),THEME:GetMetricB({group:?},{name:?}),THEME:HasMetric({group:?},{name:?})"));
            }
        }
        p.compare("local a=THEME:GetMetricNamesInGroup('Common');a[1]='changed';local b=THEME:GetMetricNamesInGroup('Common');return a~=b,table.concat(b,'|')");
    }
}

#[test]
fn strings_and_invalid_metric_arguments_preserve_missing_value_contracts() {
    let mut p = Pair::new(&context());
    for section in [
        "Difficulty",
        "CustomDifficulty",
        "OptionTitles",
        "OptionNames",
        "ThemePrefs",
        "SLPlayerOptions",
        "ScreenSelectPlayMode",
        "ScreenSelectStyle",
        "GameButton",
        "TapNoteScore",
        "TapNoteScoreFA+",
        "HoldNoteScore",
        "Stage",
        "Months",
        "ScreenGameplay",
        "difficulty",
        "optionnames",
        "Unknown",
    ] {
        p.compare(&format!(
            "local n=THEME:GetStringNamesInGroup({section:?});return n and table.concat(n,'|')"
        ));
        for name in [
            "Yes",
            "Difficulty_Hard",
            "HeaderText",
            "MusicRateMultiplier",
            "Unknown_Key",
            "",
        ] {
            p.compare(&format!(
                "return THEME:GetString({section:?},{name:?}),THEME:HasString({section:?},{name:?})"
            ));
        }
    }
    for method in [
        "GetMetric",
        "GetMetricF",
        "GetMetricI",
        "GetMetricB",
        "HasMetric",
        "GetString",
        "HasString",
        "GetMetricNamesInGroup",
        "GetStringNamesInGroup",
    ] {
        for args in [
            "nil,nil",
            "12,34",
            "'Common',{}",
            "string.char(255),'Class'",
            "'Common',string.char(255)",
        ] {
            p.compare(&format!("local result=THEME:{method}({args});return type(result)=='table' and table.concat(result,'|') or result"));
        }
    }
}

#[test]
fn lead_in_metrics_use_current_double_rate_while_definition_queries_do_not_evaluate_it() {
    let mut p = Pair::new(&context());
    p.compare("SL=false;return THEME:HasMetric('ScreenGameplay','MinSecondsToStep'),THEME:GetMetricF('ScreenGameplay','MinSecondsToStep'),THEME:GetMetricI('ScreenGameplay','MinSecondsToStep')");
    p.compare("local ok=pcall(THEME.GetMetric,'ScreenGameplay','MinSecondsToStep');return ok");
    for rate in ["1.23456789012345", "'0x1.8'", "-2", "0", "math.huge", "0/0"] {
        p.compare(&format!("SL={{Global={{ActiveModifiers={{MusicRate={rate}}}}}}};return THEME:GetMetric('ScreenGameplay','MinSecondsToStep'),THEME:GetMetric('screengameplay','minsecondstomusic'),THEME:GetMetric('ScreenGameplay','MinSecondsToStepNextSong'),THEME:GetMetricB('ScreenGameplay','MinSecondsToStep')"));
    }
    for invalid in ["nil", "false", "{}", "'bad'"] {
        p.compare(&format!("SL.Global.ActiveModifiers.MusicRate={invalid};local ok=pcall(function()THEME:GetMetric('ScreenGameplay','MinSecondsToStep')end);return ok"));
    }
}

#[test]
fn mapped_metrics_keep_arbitrary_lua_values_and_boolean_conversion() {
    let mut p = Pair::new(&context());
    p.compare("SL={Global={GameMode='custom'},Metrics={custom={}}}");
    for value in [
        "nil",
        "false",
        "true",
        "0",
        "-0",
        "0.5",
        "0/0",
        "''",
        "'false'",
        "string.char(255)",
        "{}",
        "function()end",
        "coroutine.create(function()end)",
    ] {
        p.compare(&format!("local v={value};SL.Metrics.custom.PercentScoreWeightW1=v;local actual=THEME:GetMetric('ScoreKeeperNormal','PercentScoreWeightW1');assert(actual==v or (type(v)=='number' and v~=v and actual~=actual));return type(actual),THEME:GetMetricB('ScoreKeeperNormal','PercentScoreWeightW1'),THEME:HasMetric('ScoreKeeperNormal','PercentScoreWeightW1'),THEME:GetMetricF('ScoreKeeperNormal','PercentScoreWeightW1'),THEME:GetMetric('Common','PercentScoreWeightW1')"));
    }
    p.compare("SL.Global.GameMode=2;SL.Metrics['2']={LifePercentChangeW1=.75};return THEME:GetMetric('LifeMeterBar','LifePercentChangeW1'),THEME:GetMetric('LifeMeterBar','lifepercentchangew1')");
}

#[test]
fn dynamic_metric_getters_preserve_lookup_order_reentry_and_gc_roots() {
    let mut p = Pair::new(&context());
    p.compare(
        r#"
        seen={};local function log(k)seen[#seen+1]=k;collectgarbage('collect')end
        SL=setmetatable({}, {__index=function(t,k)
            log('SL.'..k)
            if k=='Global' then return setmetatable({}, {__index=function(t,k)
                log('Global.'..k)
                if k=='GameMode' then return 'ITG' end
                if k=='ActiveModifiers' then return setmetatable({}, {__index=function(t,k)
                    log('Modifiers.'..k);return THEME:GetMetric('LifeMeterBar','InitialValue')+1.25
                end}) end
            end}) end
            if k=='Metrics' then return setmetatable({}, {__index=function(t,k)
                log('Metrics.'..k);return setmetatable({}, {__index=function(t,k)
                    log('Metric.'..k);return THEME:GetMetric('Combo','ShowComboAt')+3
                end})
            end}) end
        end})
        local weight=THEME:GetMetric('ScoreKeeperNormal','PercentScoreWeightW1')
        local lead=THEME:GetMetric('ScreenGameplay','MinSecondsToMusic')
        return weight,lead,table.concat(seen,'|')
    "#,
    );
    p.compare(r#"
        SL=setmetatable({}, {__index=function()collectgarbage('collect');error('metric failed')end})
        local ok=pcall(function()THEME:GetMetric('ScoreKeeperNormal','GradeWeightW1')end)
        SL={Global={GameMode='ITG'},Metrics={ITG={GradeWeightW1=9}}}
        return ok,THEME:GetMetric('ScoreKeeperNormal','GradeWeightW1'),THEME:HasMetric('ScreenGameplay','MinSecondsToStep')
    "#);
}

#[test]
fn unchanged_simply_love_mode_fixture_supplies_the_current_metric_values() {
    let source =
        include_str!("../../../tests/fixtures/itgmania-song-lua-micro/simply-love-mode-tables.lua");
    for rescore in [false, true] {
        let mut p = Pair::new(&context());
        p.compare(&format!("ThemePrefs={{Get=function()return {rescore} end}};fixture=(function()\n{source}\nend)();SL={{Global={{GameMode='ITG'}},Metrics=fixture.Metrics}}"));
        p.compare(r#"
            local observations={}
            for _,mode in ipairs({'Casual','ITG','FA+'}) do
                SL.Global.GameMode=mode
                local keys={};for key in pairs(fixture.Metrics[mode])do keys[#keys+1]=key end;table.sort(keys)
                for _,key in ipairs(keys)do
                    local group=(key:find('^Life')or key=='InitialValue')and'LifeMeterBar'or'ScoreKeeperNormal'
                    local value=THEME:GetMetric(group,key)
                    assert(value==fixture.Metrics[mode][key] and THEME:HasMetric(group,key))
                    observations[#observations+1]=mode..':'..key..':'..string.format('%.17g',value)
                end
            end
            return table.concat(observations,'|')
        "#);
    }
}

#[test]
fn theme_font_paths_and_metrics_feed_owned_actor_resources() {
    let source = r#"return LoadFont(THEME:GetPathF('Common','Normal'))..{
        Text=THEME:GetString('Difficulty','Difficulty_Hard'),
        InitCommand=function(self)label=self;self:zoom(THEME:GetMetricF('LifeMeterBar','InitialValue'))end}
    "#;
    for live in [false, true] {
        let mut context = context();
        context.live = live;
        let mut rust = SongRuntime::new(&context).unwrap();
        let declaration = execute(&mut rust, source)[0];
        let scene = rust.instantiate_actor(declaration).unwrap();
        let label = rust.vm_mut().get_global(b"label").unwrap();
        reference_test::assert_debug(
            "song/tests/rust_theme.rs::bitmap",
            &rust.actor_bitmap_text(label).unwrap(),
        );
        let actual: Vec<_> = rust
            .actor_scene_snapshot(scene)
            .unwrap()
            .into_iter()
            .map(|actor| format!("{:?}", actor.render))
            .collect();
        reference_test::assert_debug("song/tests/rust_theme.rs::render", &actual);
        rust.release_actor_scene(scene).unwrap();
    }
}

#[test]
fn saved_queries_follow_replaced_state_and_release_discarded_manager_tables() {
    let mut runtime = SongRuntime::new(&context()).unwrap();
    execute(
        &mut runtime,
        r#"
        local weak=setmetatable({}, {__mode='v'})
        weak.theme=THEME;query=THEME.GetMetric
        SL={Global={ActiveModifiers={MusicRate=2}}};weak.sl=SL
        THEME=nil;collectgarbage('collect');collectgarbage('collect')
        assert(weak.theme==nil and query('ScreenGameplay','MinSecondsToStep')==12)
        SL={Global={ActiveModifiers={MusicRate=1.5}}};collectgarbage('collect');collectgarbage('collect')
        assert(weak.sl==nil and query('ScreenGameplay','MinSecondsToStep')==9)
        query=nil;SL=nil;collectgarbage('collect')
    "#,
    );
}

#[test]
fn virtual_theme_actor_paths_keep_production_classes_spelling_and_fresh_identity() {
    let mut p = Pair::new(&context());
    for suffix in [
        "part",
        "part.lua",
        "model.txt",
        "part.sprite",
        "part/",
        "part.PNG",
        "part.jpg",
        "part.jpeg",
        "part.bmp",
        "part.gif",
        "part.webp",
        "part.qoi",
        "part.tif",
        "part.tiff",
        "part.mp4",
        "part.avi",
        "part.webm",
        "part.mov",
        "part.mkv",
        "part.mpg",
        "part.mpeg",
        "part.ogv",
        "part.ogg",
        "part.mp3",
        "part.wav",
        "part.flac",
        "part.opus",
        "part.m4a",
        "part.aac",
        "part.oga",
        "part.flv",
        "part.wmv",
    ] {
        for leading in ["", "///"] {
            let path = format!(
                "{leading}{}G/folder/{suffix}",
                song::SONG_LUA_THEME_PATH_PREFIX
            );
            p.compare(&format!("local a=LoadActor({path:?});local b=LoadActor({path:?});return a.Class,a.Texture,a.File,a~=b"));
            // The Rust host entry must preserve virtual paths instead of making
            // them absolute before forwarding them to the same Lua loader.
            let actor = p.rust.load_actor(Path::new(&path), &[]).unwrap()[0];
            p.rust.vm_mut().set_global(b"entry", actor).unwrap();
            execute(
                &mut p.rust,
                &format!(
                    "local a=LoadActor({path:?});assert(entry.Class==a.Class and entry.Texture==a.Texture and entry.File==a.File)"
                ),
            );
        }
    }
    p.compare("local a=LoadActor(THEME:GetPathG('','_VisualStyles/Spooky/ExtraSpooky/Bats'));return a.Class,a.Texture,a.File");
    execute(
        &mut p.rust,
        r#"
        assert(__songlua_theme_path_actor_type==nil)
        local f=LoadActorFunc(THEME:GetPathG('Common','image.png'))
        collectgarbage('collect')
        local a,b=f(),f();assert(a~=b and a.Texture==b.Texture and a.Class=='Sprite')
    "#,
    );
}

#[test]
fn virtual_theme_actors_materialize_with_owned_commands_and_render_state() {
    let source = r#"
        events={}
        local function init(self)events[#events+1]=self:GetName();self:zoom(.75)end
        return Def.ActorFrame{
            LoadActor(THEME:GetPathB('ScreenGameplay','decoration.lua'))..{Name='plain',InitCommand=init},
            LoadActor(THEME:GetPathG('Common','image.png'))..{Name='image',InitCommand=init},
            LoadActor(THEME:GetPathS('Common','sound.ogg'))..{Name='sound',InitCommand=init},
            LoadActor(THEME:GetPathG('Common','movie.webm'))..{Name='movie',InitCommand=init},
            Def.Sprite{Texture=THEME:GetPathG('Common','movie.webm'),Name='declared',InitCommand=init},
            InitCommand=function(self)root=self end
        }
    "#;
    for live in [false, true] {
        let mut context = context();
        context.live = live;
        let mut rust = SongRuntime::new(&context).unwrap();
        let declaration = execute(&mut rust, source)[0];
        let scene = rust.instantiate_actor(declaration).unwrap();
        execute(
            &mut rust,
            "collectgarbage('collect');assert(table.concat(events,',')=='plain,image,sound,movie,declared')",
        );
        let observation = "local a=root:GetChild('movie');return a:GetDecodeMovie(),rawget(a,'__songlua_state_decode_movie')";
        let actual_getter = execute(&mut rust, observation);
        assert_eq!(actual_getter.len(), 2);
        let Value::Boolean(decode) = actual_getter[0] else {
            panic!("boolean decode getter expected")
        };
        let state = match actual_getter[1] {
            Value::Nil => None,
            Value::Boolean(value) => Some(value),
            other => panic!("optional boolean state expected: {other:?}"),
        };
        reference_test::assert_source_debug(
            "song/tests/rust_theme.rs::movie_getter",
            observation,
            &(decode, state),
        );
        let actual: Vec<_> = rust
            .actor_scene_snapshot(scene)
            .unwrap()
            .into_iter()
            .map(|actor| format!("{:?}", actor.render))
            .collect();
        reference_test::assert_debug("song/tests/rust_theme.rs::render", &actual);
        rust.release_actor_scene(scene).unwrap();
        execute(&mut rust, "root=nil;events=nil;collectgarbage('collect')");
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
