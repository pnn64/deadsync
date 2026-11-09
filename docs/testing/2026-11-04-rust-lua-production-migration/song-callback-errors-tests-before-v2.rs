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
fn compile(source: &str) -> (Output, Output) {
    let dir = tempfile::tempdir().unwrap();
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
    let native = song::compile_song_lua_layers_with_default_host(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap();
    let rust = song::rust_engine::compile_prepared_song_lua_layers(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap();
    (
        native.into_iter().next().unwrap(),
        rust.into_iter().next().unwrap(),
    )
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
fn compare(mut a: Output, mut b: Output, marker: &str) -> Output {
    // Independent engines retain their own source/traceback rendering. Compare
    // every diagnostic's callback, actor, clock and order, and require the real
    // authored cause in both strings before normalizing that engine-only suffix.
    let expected = &a.info.unsupported_perframe_captures;
    let actual = &b.info.unsupported_perframe_captures;
    assert_eq!(a.info.unsupported_perframes, b.info.unsupported_perframes);
    assert_eq!(
        expected.len(),
        actual.len(),
        "native: {expected:?}; Rust: {actual:?}"
    );
    for (a, b) in expected.iter().zip(actual) {
        assert_eq!(header(a), header(b), "native: {a}; Rust: {b}");
        assert!(
            a.contains(marker) && b.contains(marker),
            "native: {a}; Rust: {b}"
        );
    }
    let original = b.info.unsupported_perframe_captures.clone();
    a.info
        .unsupported_perframe_captures
        .iter_mut()
        .for_each(|s| *s = header(s).to_owned());
    b.info
        .unsupported_perframe_captures
        .iter_mut()
        .for_each(|s| *s = header(s).to_owned());
    macro_rules! same { ($($field:ident),+) => { $(
        assert_field(stringify!($field), &format!("{:?}",a.$field), &format!("{:?}",b.$field));
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
fn assert_field(field: &str, a: &str, b: &str) {
    if a == b {
        return;
    }
    let a = a.chars().collect::<Vec<_>>();
    let b = b.chars().collect::<Vec<_>>();
    let at = a
        .iter()
        .zip(&b)
        .position(|(a, b)| a != b)
        .unwrap_or(a.len().min(b.len()));
    let snippet = |v: &[char]| {
        v[at.saturating_sub(90)..(at + 200).min(v.len())]
            .iter()
            .collect::<String>()
    };
    panic!(
        "{field}, first difference at {at}\nnative: {}\nRust: {}",
        snippet(&a),
        snippet(&b)
    );
}

#[test]
fn initial_update_errors_keep_partial_state_and_report_once() {
    let (a, b) = compile(
        r#"
        return Def.ActorFrame{Name='broken',OnCommand=function(self)
            self:SetUpdateFunction(function(self)self:x(42);collectgarbage();error('initial marker',0)end)
        end}
    "#,
    );
    let out = compare(a, b, "initial marker");
    assert_eq!(out.info.unsupported_perframes, 1);
    assert!(
        out.info.unsupported_perframe_captures[0]
            .starts_with("update function for ActorFrame 'broken' at beat 0.000:")
    );
}

#[test]
fn positive_update_errors_keep_each_actor_and_later_frames() {
    let (a, b) = compile(
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
    let out = compare(a, b, "positive marker");
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
    let (a, b) = compile(
        r#"
        return Def.ActorFrame{Name='both',OnCommand=function(self)
            self:SetUpdateFunction(function(self,dt)
                if dt>0 then self:queuecommand('Bad');self:x(7);error('double marker function',0)end
            end)
        end,BadCommand=function(self)self:y(9);error('double marker queue',0)end}
    "#,
    );
    let out = compare(a, b, "double marker");
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
    let (a, b) = compile(
        r#"
        return Def.ActorFrame{Name='cycle',OnCommand=function(self)self:queuecommand('Pulse')end,
            PulseCommand=function(self)
                self:sleep(.025):queuecommand('Pulse');self:x(GAMESTATE:GetSongBeat()*10)
                if GAMESTATE:GetSongBeat()>0 then error('cycle marker',0)end
            end,
        }
    "#,
    );
    let out = compare(a, b, "cycle marker");
    assert!(out.info.unsupported_perframes > 0);
    assert!(
        out.info
            .unsupported_perframe_captures
            .iter()
            .any(|s| s.starts_with("Pulse for ActorFrame 'cycle'"))
    );
}

#[test]
fn draw_errors_keep_partial_output_and_report_the_compiler_root_once() {
    let (a, b) = compile(
        r#"
        local q=Def.Quad{Name='quad'}
        return Def.ActorFrame{Name='draw',InitCommand=function(self)
            self:SetDrawFunction(function(self)q:Draw();error('draw marker',0)end)
        end,q}
    "#,
    );
    let out = compare(a, b, "draw marker");
    assert_eq!(out.info.unsupported_perframes, 1);
    assert!(
        out.info.unsupported_perframe_captures[0]
            .starts_with("draw callback for Actor at beat 0.000:")
    );
    assert!(!out.draw_frames.is_empty());
}

#[test]
fn authored_flag_reset_reenables_callback_reporting() {
    let (a, b) = compile(
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
    let out = compare(a, b, "reset marker");
    assert_eq!(out.info.unsupported_perframes, 2);
}

#[test]
fn framebuffer_script_notices_do_not_become_callback_failures() {
    let (a, b) = compile(
        r#"
        return Def.ActorFrame{Def.ActorFrameTexture{Name='bad',InitCommand=function(self)
            self:SetWidth(0):SetHeight(8):Create()
        end}}
    "#,
    );
    let out = compare(a, b, "");
    assert_eq!(out.info.unsupported_perframes, 0);
}
