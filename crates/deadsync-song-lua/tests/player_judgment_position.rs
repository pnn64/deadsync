#![cfg(feature = "test-support")]

use deadlib_lua::compile_named;
use deadsync_song_lua::{ActorScene, SongLayerPlan, SongLuaCompileContext, SongRuntime};

struct Gameplay {
    runtime: SongRuntime,
    screen: ActorScene,
    _directory: tempfile::TempDir,
}

impl Gameplay {
    fn new() -> Self {
        Self::with_second_player(true)
    }

    fn with_second_player(enabled: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("default.lua");
        std::fs::write(
            &path,
            "return Def.ActorFrame { InitCommand=function(self) observer=self end }",
        )
        .unwrap();
        let mut context = SongLuaCompileContext::new(directory.path(), "judgment position");
        context.live = true;
        context.style_name = if enabled { "versus" } else { "single" }.into();
        context.players[1].enabled = enabled;
        context.music_length_seconds = 10.;
        let mut runtime = SongRuntime::new(&context).unwrap();
        runtime.begin_song_compilation().unwrap();
        let (screen, selected) = runtime.construct_gameplay_screen(&context).unwrap();
        runtime.load_gameplay_theme(screen, &selected).unwrap();
        let plan = SongLayerPlan::new(&[&path], 0, &selected).unwrap();
        runtime
            .initialize_song_layers(screen, &plan)
            .unwrap()
            .unwrap();
        runtime
            .activate_song_layers(screen, &selected)
            .unwrap()
            .unwrap();
        runtime.begin_actor_playback();
        let mut game = Self {
            runtime,
            screen,
            _directory: directory,
        };
        game.run(
            r#"
            p=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
            q=SCREENMAN:GetTopScreen():GetChild('PlayerP2') or __songlua_top_screen_player_2
            j=p:GetChild('Judgment'); k=q:GetChild('Judgment')
            options=GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Current')
            other=GAMESTATE:GetPlayerState(1):GetPlayerOptions('ModsLevel_Current')
            "#,
        );
        game
    }

    fn run(&mut self, source: &str) {
        let code = compile_named(source.as_bytes(), b"@/judgment-position-check.lua").unwrap();
        self.runtime
            .vm_mut()
            .execute(&code)
            .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }

    fn advance(&mut self, seconds: f64) {
        assert!(
            self.runtime
                .advance_song_live_actors(self.screen, seconds, None)
                .unwrap()
        );
        let diagnostics = self.runtime.take_actor_diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}

impl Drop for Gameplay {
    fn drop(&mut self) {
        self.runtime.end_actor_scene(self.screen).unwrap();
        self.runtime.release_actor_scene(self.screen).unwrap();
    }
}

#[test]
fn judgment_position_uses_exact_reverse_and_current_mini_tiny() {
    let mut game = Gameplay::new();
    game.run(
        r#"
        -- Player::Init retains the new actor's position until Player::Update.
        assert(j:GetY()==0 and k:GetY()==0)
        options:Reverse(1); options:Centered(1.75); options:Mini(1); options:Tiny(1)
        other:Reverse(.75); other:Centered(-.5); other:Mini(-2)
        j:basezoom(3); k:basezoom(4)
        "#,
    );
    game.advance(0.25);
    game.run(
        r#"
        assert(j:GetY()==30 and j:GetDestY()==30 and j:GetZoom()==.25)
        assert(k:GetY()==-30 and k:GetDestY()==-30 and k:GetZoom()==1)
        assert(j:GetBaseZoomX()==3 and k:GetBaseZoomX()==4)
        options:Reverse(1.00001); options:Mini(0); options:Tiny(0)
        other:Reverse(3)
        "#,
    );
    game.advance(0.25);
    game.run(
        r#"
        assert(j:GetDestY()==-30 and j:GetZoom()==1)
        assert(k:GetDestY()==30)
        options:Reverse(.5); options:Reverse1(.5)
        other:Reverse(-1)
        "#,
    );
    game.advance(0.25);
    game.run("assert(j:GetDestY()==30 and k:GetDestY()==-30)");
}

#[test]
fn judgment_engine_write_requires_a_player_attached_to_gameplay() {
    for enabled in [false, true] {
        let mut game = Gameplay::with_second_player(enabled);
        game.run(if enabled {
            "assert(SCREENMAN:GetTopScreen():GetChild('PlayerP2')==q)"
        } else {
            "assert(SCREENMAN:GetTopScreen():GetChild('PlayerP2')==nil)"
        });
        game.run(
            r#"
            assert(j:GetY()==0 and k:GetY()==0 and k:GetZoom()==1)
            other:Reverse(1); other:Mini(2)
            -- Visibility controls drawing, not an attached Player's update.
            q:visible(false)
            "#,
        );
        game.advance(0.125);
        game.run("assert(j:GetY()==-30 and j:GetDestY()==-30)");
        game.run(if enabled {
            "assert(k:GetY()==30 and k:GetDestY()==30 and k:GetZoom()==.25)"
        } else {
            "assert(k:GetY()==0 and k:GetDestY()==0 and k:GetZoom()==1)"
        });
    }
}

#[test]
fn judgment_engine_write_replaces_only_the_last_tween_destination() {
    let mut game = Gameplay::new();
    game.run(
        r#"
        j:xy(20,70):zoom(2):rotationz(40):basezoom(3)
        j:linear(2):xy(40,100):zoom(3)
        j:linear(2):xy(80,200):zoom(4)
        assert(j:GetY()==70 and j:GetDestY()==200 and j:GetTweenTimeLeft()==4)
        "#,
    );
    game.advance(0.5);
    game.run(
        r#"
        -- The first tween has already advanced; only the tail is replaced.
        assert(j:GetX()==25 and j:GetY()==77.5,
            string.format('current=(%g,%g), destination=(%g,%g), zoom=%g, rotation=%g, time=%g',
                j:GetX(),j:GetY(),j:GetDestX(),j:GetDestY(),j:GetZoom(),j:GetRotationZ(),j:GetTweenTimeLeft()))
        assert(j:GetDestX()==0 and j:GetDestY()==-30)
        assert(j:GetZoom()==1 and j:GetRotationZ()==0 and j:GetBaseZoomX()==3)
        assert(j:GetTweenTimeLeft()==3.5)
        "#,
    );
    game.advance(1.5);
    game.run("assert(j:GetY()==100 and j:GetDestY()==-30 and j:GetTweenTimeLeft()==2)");
    game.advance(1.0);
    game.run("assert(j:GetY()==35 and j:GetDestY()==-30 and j:GetTweenTimeLeft()==1)");
    game.advance(1.0);
    game.run("assert(j:GetY()==-30 and j:GetDestY()==-30 and j:GetTweenTimeLeft()==0)");
}

#[test]
fn judgment_position_is_updated_after_player_callbacks_and_before_song_callbacks() {
    let mut game = Gameplay::new();
    game.run(
        r#"
        seen={}; j:y(17)
        j:SetUpdateFunction(function(self) seen.child=self:GetDestY() end)
        p:SetUpdateFunction(function() seen.player=j:GetDestY() end)
        observer:SetUpdateFunction(function()
            local y=j:GetDestY()
            seen.song=y
            -- Dyscontrolled Galaxy's judzoom2 preserves the native destination
            -- while replacing the scale and restarting a one-second sleep.
            j:stoptweening():zoom(1.25):y(y):sleep(1)
        end)
        options:Reverse(1)
        "#,
    );
    game.advance(0.25);
    game.run(
        r#"
        assert(seen.child==17 and seen.player==17 and seen.song==30)
        assert(j:GetY()==30 and j:GetDestY()==30 and j:GetZoom()==1.25)
        assert(j:GetTweenTimeLeft()==1)
        options:Reverse(0)
        "#,
    );
    game.advance(0.25);
    game.run(
        r#"
        assert(seen.child==30 and seen.player==30 and seen.song==-30)
        assert(j:GetY()==-30 and j:GetDestY()==-30 and j:GetZoom()==1.25)
        assert(j:GetTweenTimeLeft()==1)
        "#,
    );
}
