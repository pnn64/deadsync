use deadsync_song_lua::{
    self as song, CompiledSongLua, SongLuaCompileContext, SongLuaOverlayActor, SongLuaOverlayKind,
};
use std::{path::Path, sync::Arc};

type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
type Output = CompiledSongLua<SongLuaOverlayActor<Kind>>;
fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}
fn compile_results(source: &str) -> Result<Vec<Output>, String> {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    let path = dir.path().join("default.lua");
    std::fs::write(&path, source).unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "callback diagnostics");
    context.players[1].enabled = false;
    context.music_length_seconds = 0.2;
    context.compile_clock_inputs = (0..=12)
        .map(|i| {
            [
                i as f64 / 60.,
                i as f64 / 60.,
                if i == 0 { 0. } else { f64::from(1.0_f32 / 60.) },
            ]
        })
        .collect();

    let rust = song::compile_prepared_song_lua_layers(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    );
    rust
}
fn compile(source: &str) -> Output {
    compile_results(source).unwrap().into_iter().next().unwrap()
}
fn header(detail: &str) -> &str {
    let at = detail
        .find(" at beat ")
        .expect("callback error identifies its beat");
    let end = at
        + detail[at..]
            .find(": ")
            .expect("callback error separates its cause");
    &detail[..end]
}
fn compare(mut b: Output, marker: &str) -> Output {
    // Check the authored causes before normalizing engine-specific traceback text.
    for detail in &b.info.unsupported_perframe_captures {
        assert!(detail.contains(marker), "{detail}");
    }
    let original = b.info.unsupported_perframe_captures.clone();

    b.info
        .unsupported_perframe_captures
        .iter_mut()
        .for_each(|s| *s = header(s).to_owned());
    macro_rules! same { ($($field:ident),+) => { $(
        assert_field(stringify!($field), &b.$field);
    )+ } }
    same!(
        live_indices,
        loaded_steps,
        loaded_style,
        startup_reloads,
        startup,
        entry_path,
        screen_width,
        screen_height,
        beat_mods,
        time_mods,
        eases,
        messages,
        sound_paths,
        overlays,
        draw_frames,
        screen_overlay_index,
        overlay_eases,
        overlay_update_unit,
        overlay_updates,
        stateful_message_captures,
        player_actors,
        song_foreground,
        screen_layers,
        hidden_players,
        note_hides,
        column_offsets,
        column_splines,
        column_spline_origin,
        info
    );
    b.info.unsupported_perframe_captures = original;
    b
}
fn assert_field(field: &str, actual: &impl std::fmt::Debug) {
    reference_test::assert_debug(
        "song/tests/rust_song_diagnostics.rs::fields",
        &(field, actual),
    );
}

#[test]
fn initial_update_errors_keep_partial_state_and_report_once() {
    let b = compile(
        r#"
        return Def.ActorFrame{Name='broken',OnCommand=function(self)
            self:SetUpdateFunction(function(self)self:x(42);collectgarbage();error('initial marker',0)end)
        end}
    "#,
    );
    let out = compare(b, "initial marker");
    assert_eq!(out.info.unsupported_perframes, 1);
    assert!(
        out.info.unsupported_perframe_captures[0]
            .starts_with("update function for ActorFrame 'broken' at beat 0.000:")
    );
}

#[test]
fn positive_update_errors_keep_each_actor_and_later_frames() {
    let b = compile(
        r#"
        local function init(self)
            self:SetUpdateFunction(function(self,dt)
                self:x(GAMESTATE:GetSongBeat()*20)
                if dt>0 then error('positive marker',0)end
            end)
        end
        return Def.ActorFrame{Def.ActorFrame{Name='first',OnCommand=init},Def.ActorFrame{Name='second',OnCommand=init}}
    "#,
    );
    let out = compare(b, "positive marker");
    assert_eq!(out.info.unsupported_perframes, 2);
    assert!(
        out.info
            .unsupported_perframe_captures
            .iter()
            .all(|s| s.starts_with("update function for ActorFrame '"))
    );
    assert!(!out.overlay_updates.is_empty());
}

#[test]
fn callback_and_queue_failures_use_independent_report_once_flags() {
    let b = compile(
        r#"
        return Def.ActorFrame{Name='both',OnCommand=function(self)
            self:SetUpdateFunction(function(self,dt)
                if dt>0 then self:queuecommand('Bad');self:x(7);error('double marker function',0)end
            end)
        end,BadCommand=function(self)self:y(9);error('double marker queue',0)end}
    "#,
    );
    let out = compare(b, "double marker");
    assert_eq!(out.info.unsupported_perframes, 2);
    assert!(
        out.info.unsupported_perframe_captures[0]
            .starts_with("update function for ActorFrame 'both'")
    );
    assert!(
        out.info.unsupported_perframe_captures[1].starts_with("update queue for ActorFrame 'both'")
    );
}

#[test]
fn queued_and_recurring_failures_preserve_dispatch_frames_and_command_labels() {
    let b = compile(
        r#"
        local started=false
        return Def.ActorFrame{Name='cycle',OnCommand=function(self)
                self:SetUpdateFunction(function(self,dt)
                    if dt>0 and not started then started=true;self:queuecommand('Pulse')end
                end)
            end,
            PulseCommand=function(self)
                self:sleep(.025):queuecommand('Pulse');self:x(GAMESTATE:GetSongBeat()*10)
                if GAMESTATE:GetSongBeat()>0 then error('cycle marker',0)end
            end,
        }
    "#,
    );
    let out = compare(b, "cycle marker");
    assert!(out.info.unsupported_perframes > 0);
    assert!(
        out.info
            .unsupported_perframe_captures
            .iter()
            .any(|s| s.starts_with("PulseCommand for ActorFrame 'cycle'"))
    );
}

#[test]
fn draw_errors_keep_partial_output_and_report_the_compiler_root_once() {
    let b = compile(
        r#"
        local q=Def.Quad{Name='quad'}
        return Def.ActorFrame{Name='draw',InitCommand=function(self)
            self:SetDrawFunction(function(self)q:Draw();error('draw marker',0)end)
        end,q}
    "#,
    );
    let out = compare(b, "draw marker");
    assert_eq!(out.info.unsupported_perframes, 1);
    assert!(
        out.info.unsupported_perframe_captures[0]
            .starts_with("draw callback for Actor at beat 0.000:")
    );
    assert!(!out.draw_frames.is_empty());
}

#[test]
fn authored_flag_reset_reenables_callback_reporting() {
    let b = compile(
        r#"
        local count=0
        return Def.ActorFrame{Name='reset',OnCommand=function(self)
            self:SetUpdateFunction(function(self)
                count=count+1;self:x(count)
                if count==4 then self.__songlua_update_function_error_reported=false end
                error('reset marker',0)
            end)
        end}
    "#,
    );
    let out = compare(b, "reset marker");
    assert_eq!(out.info.unsupported_perframes, 2);
}

#[test]
fn framebuffer_script_notices_do_not_become_callback_failures() {
    let b = compile(
        r#"
        return Def.ActorFrame{Def.ActorFrameTexture{Name='bad',InitCommand=function(self)
            self:SetWidth(0):SetHeight(8):Create()
        end}}
    "#,
    );
    let out = compare(b, "");
    assert_eq!(out.info.unsupported_perframes, 0);
}

#[test]
fn startup_tween_queue_failure_still_rejects_preparation() {
    let rust = compile_results(
        r#"
        return Def.ActorFrame{Name='cycle',OnCommand=function(self)self:queuecommand('Pulse')end,
            PulseCommand=function(self)
                self:sleep(.025):queuecommand('Pulse');self:x(GAMESTATE:GetSongBeat()*10)
                if GAMESTATE:GetSongBeat()>0 then error('cycle marker',0)end
            end,
        }
    "#,
    );

    assert!(rust.is_err(), "Rust must reject the startup queue failure");

    assert!(rust.unwrap_err().contains("cycle marker"));
}

#[test]
fn ordinary_report_flags_and_labels_can_reenter_and_collect() {
    let b = compile(
        r#"
        return Def.ActorFrame{Name='before',OnCommand=function(self)
            local previous=getmetatable(self)
            self.Name=nil
            setmetatable(self,{
                __index=function(t,k)
                    if k=='Name' then collectgarbage();return 'reported' end
                    if k=='__songlua_update_function_error_reported' then collectgarbage();return false end
                    local lookup=previous.__index
                    if type(lookup)=='function' then return lookup(t,k) else return lookup[k] end
                end,
                __newindex=function(t,k,v)
                    if k=='__songlua_update_function_error_reported' then rawset(t,k,v);collectgarbage();return end
                    local assign=previous.__newindex
                    if type(assign)=='function' then return assign(t,k,v) else rawset(t,k,v) end
                end,
            })
            self:SetUpdateFunction(function(self)self:x(17);error('collect marker',0)end)
        end}
    "#,
    );
    let out = compare(b, "collect marker");
    assert_eq!(out.info.unsupported_perframes, 1);
    assert!(
        out.info.unsupported_perframe_captures[0]
            .starts_with("update function for ActorFrame 'reported' at beat 0.000:")
    );
}

#[test]
fn truthy_report_flags_suppress_errors_without_stopping_callback_writes() {
    let b = compile(
        r#"
        return Def.ActorFrame{Name='suppressed',OnCommand=function(self)
            self.__songlua_update_function_error_reported=0
            self:SetUpdateFunction(function(self)self:x(23);error('suppressed marker',0)end)
        end}
    "#,
    );
    let out = compare(b, "suppressed marker");
    assert_eq!(out.info.unsupported_perframes, 0);
    assert!(
        out.overlays
            .iter()
            .any(|r| r.name.as_deref() == Some("suppressed") && r.initial_state.x == 23.)
    );
}

#[test]
fn startup_recurring_commands_without_errors_keep_chronological_output() {
    let b = compile(
        r#"
        return Def.ActorFrame{Name='cycle',OnCommand=function(self)self:queuecommand('Pulse')end,
            PulseCommand=function(self)
                self:sleep(.025):queuecommand('Pulse');self:x(GAMESTATE:GetSongBeat()*10)
            end,
        }
    "#,
    );
    let out = compare(b, "");
    assert_eq!(out.info.unsupported_perframes, 0);
    assert!(!out.overlay_updates.is_empty());
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
