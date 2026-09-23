use super::*;
pub(crate) type SongLuaOverlayEaseWindowRuntime =
    deadsync_gameplay::SongLuaOverlayEaseWindowRuntime<SongLuaRuntimeOverlayStateDelta>;

#[derive(Clone, Debug)]
pub(crate) struct GameplayCompiledSongLua<S> {
    pub(crate) compiled: CompiledSongLua<S>,
    pub(crate) compile_ms: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct GameplaySongLuaLayer<S> {
    pub(crate) start_beat: f32,
    pub(crate) compiled: CompiledSongLua<S>,
}

/// CPU-only song-Lua compilation prepared before Gameplay owns the screen.
///
/// The shell may build this on a worker during Select Music's options prompt,
/// then hand it to the shared gameplay runtime builder.
#[derive(Clone, Debug)]
pub struct PreparedGameplaySongLua<S> {
    pub(crate) primary: Option<GameplayCompiledSongLua<S>>,
    pub(crate) background_layers: Vec<GameplaySongLuaLayer<S>>,
    pub(crate) foreground_layers: Vec<GameplaySongLuaLayer<S>>,
}

impl<S> PreparedGameplaySongLua<S> {
    pub fn startup(&self) -> crate::SongLuaStartup {
        let mut startup = crate::SongLuaStartup::default();
        for compiled in self
            .background_layers
            .iter()
            .map(|layer| &layer.compiled)
            .chain(self.foreground_layers.iter().map(|layer| &layer.compiled))
            .chain(self.primary.iter().map(|primary| &primary.compiled))
        {
            for (target, skin) in startup
                .noteskins
                .iter_mut()
                .zip(&compiled.startup.noteskins)
            {
                if skin.is_some() {
                    target.clone_from(skin);
                }
            }
            if let Some(seconds) = compiled.startup.min_seconds_to_music {
                startup.min_seconds_to_music =
                    Some(startup.min_seconds_to_music.unwrap_or(0.0).max(seconds));
            }
            startup.hide_in |= compiled.startup.hide_in;
        }
        startup
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SongLuaSoundEvent {
    second: f32,
    path: PathBuf,
}

impl<S: Clone + std::fmt::Debug>
    deadsync_gameplay::SongLuaRuntimeBuilder<
        SongLuaOverlayActor<S>,
        SongLuaCapturedActor,
        SongLuaRuntimeOverlayStateDelta,
    > for PreparedGameplaySongLua<S>
{
    fn build_song_lua_runtime(
        self,
        params: deadsync_gameplay::SongLuaRuntimeWindowBuild<'_>,
    ) -> deadsync_gameplay::SongLuaRuntimeBuildOutput<
        SongLuaOverlayActor<S>,
        SongLuaCapturedActor,
        SongLuaRuntimeOverlayStateDelta,
    > {
        build_song_lua_runtime_windows_for_data(params, self)
    }
}
fn compile_primary_song_lua<S: Clone + std::fmt::Debug>(
    song_title: &str,
    path: &Path,
    context: &crate::SongLuaCompileContext,
    compile_song_lua: &impl Fn(
        &Path,
        &crate::SongLuaCompileContext,
    ) -> Result<CompiledSongLua<S>, String>,
) -> Option<GameplayCompiledSongLua<S>> {
    let compile_started = Instant::now();
    match compile_song_lua(path, context) {
        Ok(compiled) => Some(GameplayCompiledSongLua {
            compiled,
            compile_ms: compile_started.elapsed().as_secs_f64() * 1000.0,
        }),
        Err(err) => {
            log::warn!(
                "Failed to compile gameplay lua for '{}' from '{}': {}",
                song_title,
                path.display(),
                err,
            );
            None
        }
    }
}

fn compile_song_lua_layer<S: Clone + std::fmt::Debug>(
    song_title: &str,
    path: &Path,
    start_beat: f32,
    label: &str,
    context: &crate::SongLuaCompileContext,
    compile_song_lua: &impl Fn(
        &Path,
        &crate::SongLuaCompileContext,
    ) -> Result<CompiledSongLua<S>, String>,
) -> Option<GameplaySongLuaLayer<S>> {
    match compile_song_lua(path, context) {
        Ok(compiled) => Some(GameplaySongLuaLayer {
            start_beat,
            compiled,
        }),
        Err(err) => {
            log::warn!(
                "Failed to compile {} for '{}' from '{}': {}",
                label,
                song_title,
                path.display(),
                err,
            );
            None
        }
    }
}

#[derive(Clone, Copy)]
enum SongLuaLayerTarget {
    Primary,
    Background(f32),
    Foreground(f32),
}

fn compile_song_lua_session<S: Clone + std::fmt::Debug>(
    song: &SongData,
    primary_ix: Option<usize>,
    context: &crate::SongLuaCompileContext,
    compile_song_lua_layers: &impl Fn(
        &[&Path],
        usize,
        &crate::SongLuaCompileContext,
    ) -> Result<Vec<CompiledSongLua<S>>, String>,
) -> Result<PreparedGameplaySongLua<S>, String> {
    let mut paths = Vec::new();
    let mut targets = Vec::new();
    for change in &song.background_lua_changes {
        paths.push(change.path.as_path());
        targets.push(SongLuaLayerTarget::Background(change.start_beat));
    }
    for (index, change) in song.foreground_lua_changes.iter().enumerate() {
        if !change.path.is_file() {
            continue;
        }
        paths.push(change.path.as_path());
        targets.push(if Some(index) == primary_ix {
            SongLuaLayerTarget::Primary
        } else {
            SongLuaLayerTarget::Foreground(change.start_beat)
        });
    }
    let primary_index = targets
        .iter()
        .position(|target| matches!(target, SongLuaLayerTarget::Primary))
        .unwrap_or(0);
    if paths.is_empty() {
        return Ok(PreparedGameplaySongLua::default());
    }
    let compile_started = Instant::now();
    let compiled = compile_song_lua_layers(&paths, primary_index, context)?;
    if compiled.len() != targets.len() {
        return Err("song lua session returned the wrong layer count".to_string());
    }
    let compile_ms = compile_started.elapsed().as_secs_f64() * 1000.0;
    let mut data = PreparedGameplaySongLua::default();
    for (compiled, target) in compiled.into_iter().zip(targets) {
        match target {
            SongLuaLayerTarget::Primary => {
                data.primary = Some(GameplayCompiledSongLua {
                    compiled,
                    compile_ms,
                });
            }
            SongLuaLayerTarget::Background(start_beat) => {
                data.background_layers.push(GameplaySongLuaLayer {
                    start_beat,
                    compiled,
                })
            }
            SongLuaLayerTarget::Foreground(start_beat) => {
                data.foreground_layers.push(GameplaySongLuaLayer {
                    start_beat,
                    compiled,
                })
            }
        }
    }
    Ok(data)
}

fn compile_song_lua_separately<S: Clone + std::fmt::Debug>(
    song: &SongData,
    primary_ix: usize,
    context: &crate::SongLuaCompileContext,
    compile_song_lua: &impl Fn(
        &Path,
        &crate::SongLuaCompileContext,
    ) -> Result<CompiledSongLua<S>, String>,
) -> PreparedGameplaySongLua<S> {
    let primary = compile_primary_song_lua(
        song.title.as_str(),
        &song.foreground_lua_changes[primary_ix].path,
        context,
        &compile_song_lua,
    );
    let background_layers = song
        .background_lua_changes
        .iter()
        .filter_map(|change| {
            compile_song_lua_layer(
                song.title.as_str(),
                &change.path,
                change.start_beat,
                "background lua layer",
                context,
                &compile_song_lua,
            )
        })
        .collect();
    let foreground_layers = song
        .foreground_lua_changes
        .iter()
        .enumerate()
        .filter(|(index, change)| *index != primary_ix && change.path.is_file())
        .filter_map(|(_, change)| {
            compile_song_lua_layer(
                song.title.as_str(),
                &change.path,
                change.start_beat,
                "foreground lua layer",
                context,
                &compile_song_lua,
            )
        })
        .collect();
    PreparedGameplaySongLua {
        primary,
        background_layers,
        foreground_layers,
    }
}

fn compile_song_lua_without_primary<S: Clone + std::fmt::Debug>(
    song: &SongData,
    context: &crate::SongLuaCompileContext,
    compile_song_lua: &impl Fn(
        &Path,
        &crate::SongLuaCompileContext,
    ) -> Result<CompiledSongLua<S>, String>,
) -> PreparedGameplaySongLua<S> {
    let background_layers = song
        .background_lua_changes
        .iter()
        .filter_map(|change| {
            compile_song_lua_layer(
                song.title.as_str(),
                &change.path,
                change.start_beat,
                "background lua layer",
                context,
                &compile_song_lua,
            )
        })
        .collect();
    let foreground_layers = song
        .foreground_lua_changes
        .iter()
        .filter(|change| change.path.is_file())
        .filter_map(|change| {
            compile_song_lua_layer(
                song.title.as_str(),
                &change.path,
                change.start_beat,
                "foreground lua layer",
                context,
                &compile_song_lua,
            )
        })
        .collect();
    PreparedGameplaySongLua {
        primary: None,
        background_layers,
        foreground_layers,
    }
}

/// Compile all song layers with one session, retaining the independent-layer fallback.
pub fn prepare_song_lua<S: Clone + std::fmt::Debug>(
    song: &SongData,
    context: &crate::SongLuaCompileContext,
    compile_song_lua: impl Fn(
        &Path,
        &crate::SongLuaCompileContext,
    ) -> Result<CompiledSongLua<S>, String>,
    compile_song_lua_layers: impl Fn(
        &[&Path],
        usize,
        &crate::SongLuaCompileContext,
    ) -> Result<Vec<CompiledSongLua<S>>, String>,
) -> PreparedGameplaySongLua<S> {
    let primary_ix = song
        .foreground_lua_changes
        .iter()
        .position(|change| change.start_beat <= 0.0 && change.path.is_file());
    if primary_ix.is_none()
        && song.background_lua_changes.is_empty()
        && song.foreground_lua_changes.is_empty()
    {
        return PreparedGameplaySongLua::default();
    }

    compile_song_lua_session(song, primary_ix, context, &compile_song_lua_layers).unwrap_or_else(|err| {
        log::warn!(
            "Failed to compile shared gameplay lua session for '{}': {}; retrying layers independently",
            song.title,
            err,
        );
        primary_ix.map_or_else(
            || compile_song_lua_without_primary(song, &context, &compile_song_lua),
            |primary_ix| compile_song_lua_separately(song, primary_ix, &context, &compile_song_lua),
        )
    })
}

pub fn song_lua_sound_paths<S: Clone + std::fmt::Debug>(
    data: &PreparedGameplaySongLua<S>,
) -> Vec<PathBuf> {
    crate::compiled_song_lua_sound_paths(
        data.primary
            .iter()
            .map(|primary| &primary.compiled)
            .chain(data.background_layers.iter().map(|layer| &layer.compiled))
            .chain(data.foreground_layers.iter().map(|layer| &layer.compiled)),
    )
}

pub(crate) fn song_lua_video_paths<CapturedActor, StateDelta, S: Clone>(
    visuals: &SongLuaRuntimeVisuals<SongLuaOverlayActor<S>, CapturedActor, StateDelta>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    crate::push_song_lua_video_paths(&visuals.overlays, &mut seen, &mut paths);
    for layer in &visuals.background_visual_layers {
        crate::push_song_lua_video_paths(&layer.overlays, &mut seen, &mut paths);
    }
    for layer in &visuals.foreground_visual_layers {
        crate::push_song_lua_video_paths(&layer.overlays, &mut seen, &mut paths);
    }
    paths
}

fn push_song_lua_sound_events<S: Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    out: &mut Vec<SongLuaSoundEvent>,
) {
    for (overlay_index, overlay) in overlays.iter().enumerate() {
        let SongLuaOverlayKind::Sound { sound_path } = &overlay.kind else {
            continue;
        };
        let Some(events) = overlay_events.get(overlay_index) else {
            continue;
        };
        for event in events {
            let Some(command) = overlay.message_commands.get(event.command_index) else {
                continue;
            };
            for block in &command.blocks {
                if block.delta.sound_play != Some(true) {
                    continue;
                }
                let second = event.event_second + block.start;
                if second.is_finite() {
                    out.push(SongLuaSoundEvent {
                        second,
                        path: sound_path.clone(),
                    });
                }
            }
        }
    }
}

pub(crate) fn song_lua_sound_events<CapturedActor, StateDelta, S: Clone>(
    visuals: &SongLuaRuntimeVisuals<SongLuaOverlayActor<S>, CapturedActor, StateDelta>,
) -> Vec<SongLuaSoundEvent> {
    let mut events = Vec::new();
    push_song_lua_sound_events(&visuals.overlays, &visuals.overlay_events, &mut events);
    for layer in &visuals.background_visual_layers {
        push_song_lua_sound_events(&layer.overlays, &layer.overlay_events, &mut events);
    }
    for layer in &visuals.foreground_visual_layers {
        push_song_lua_sound_events(&layer.overlays, &layer.overlay_events, &mut events);
    }
    events.sort_by(|a, b| a.second.total_cmp(&b.second));
    events
}

fn build_song_lua_compiled_visual_layer_runtime<S: Clone + std::fmt::Debug>(
    song_title: &str,
    start_beat: f32,
    compiled: &CompiledSongLua<S>,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
) -> Option<
    deadsync_gameplay::SongLuaVisualLayerRuntime<
        SongLuaOverlayActor<S>,
        SongLuaCapturedActor,
        SongLuaRuntimeOverlayStateDelta,
    >,
> {
    let start_second = deadsync_gameplay::song_lua_time_to_second_like(
        deadsync_gameplay::SongLuaRuntimeTimeUnit::Beat,
        start_beat,
        timing_player,
        global_offset_seconds,
    );
    if !start_second.is_finite() {
        log::warn!(
            "Skipping song lua visual layer for '{song_title}' at beat {start_beat:.3}: invalid start time"
        );
        return None;
    }

    let message_seconds = deadsync_gameplay::build_song_lua_message_seconds(
        compiled.messages.iter().map(|message| message.beat),
        timing_player,
        global_offset_seconds,
    );
    let overlay_events = crate::gameplay::build_song_lua_overlay_message_events_with_seconds(
        compiled,
        &message_seconds,
    );
    let overlay_eases = crate::gameplay::build_song_lua_overlay_ease_windows_with_events(
        compiled,
        timing_player,
        global_offset_seconds,
        &overlay_events,
    );
    let song_foreground_events = crate::gameplay::build_song_lua_actor_message_events_for_commands(
        &compiled.messages,
        &message_seconds,
        &compiled.song_foreground.message_commands,
    );
    let overlays = song_lua_runtime_overlays(compiled, timing_player, global_offset_seconds);

    Some(deadsync_gameplay::build_song_lua_visual_layer_runtime(
        start_second,
        compiled.screen_width,
        compiled.screen_height,
        overlays,
        overlay_eases,
        overlay_events,
        compiled.song_foreground.clone(),
        song_foreground_events,
    ))
}

fn song_lua_runtime_overlays<S: Clone + std::fmt::Debug>(
    compiled: &CompiledSongLua<S>,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
) -> Vec<SongLuaOverlayActor<S>> {
    let mut overlays = compiled.overlays.clone();
    let tracks = crate::gameplay::build_song_lua_overlay_update_tracks(
        compiled,
        timing_player,
        global_offset_seconds,
    );
    if !tracks.is_empty() {
        overlays.push(SongLuaOverlayActor {
            kind: SongLuaOverlayKind::UpdateTracks { tracks },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        });
    }
    overlays
}

fn log_song_lua_runtime_debug<S: Clone + std::fmt::Debug>(
    song_title: &str,
    compiled: &CompiledSongLua<S>,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    messages: &[crate::SongLuaMessageEvent],
    hidden_players: &[bool; MAX_PLAYERS],
    total_constant: usize,
    total_eases: usize,
    total_column_offsets: usize,
    unsupported_targets: usize,
) {
    log::debug!(
        "Song lua runtime detail for '{}': entry='{}' screen_space={:.1}x{:.1} hidden_players={:?} constants={} eases={} column_offsets={} overlay_eases={} overlays={} messages={} sound_assets={} unsupported_targets={} unsupported_function_eases={} unsupported_function_actions={} unsupported_perframes={} skipped_message_commands={}",
        song_title,
        compiled.entry_path.display(),
        compiled.screen_width,
        compiled.screen_height,
        hidden_players,
        total_constant,
        total_eases,
        total_column_offsets,
        overlay_eases.len(),
        compiled.overlays.len(),
        messages.len(),
        compiled.sound_paths.len(),
        unsupported_targets,
        compiled.info.unsupported_function_eases,
        compiled.info.unsupported_function_actions,
        compiled.info.unsupported_perframes,
        compiled.info.skipped_message_command_captures.len(),
    );

    let mut message_counts = std::collections::BTreeMap::<&str, usize>::new();
    for event in messages {
        *message_counts.entry(event.message.as_str()).or_default() += 1;
    }
    if !message_counts.is_empty() {
        log::debug!(
            "Song lua message kinds for '{}': {}",
            song_title,
            message_counts
                .iter()
                .map(|(message, count)| format!("{message}x{count}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !compiled.sound_paths.is_empty() {
        log::debug!(
            "Song lua sound assets for '{}': {}",
            song_title,
            compiled
                .sound_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    if !compiled.info.skipped_message_command_captures.is_empty() {
        log::debug!(
            "Song lua skipped message command captures for '{}': {}",
            song_title,
            compiled.info.skipped_message_command_captures.join(" | ")
        );
    }
    if !compiled
        .info
        .unsupported_function_action_captures
        .is_empty()
    {
        log::debug!(
            "Song lua unsupported function action captures for '{}': {}",
            song_title,
            compiled
                .info
                .unsupported_function_action_captures
                .join(" | ")
        );
    }
    if !compiled.info.unsupported_function_ease_captures.is_empty() {
        log::debug!(
            "Song lua unsupported function ease captures for '{}': {}",
            song_title,
            compiled.info.unsupported_function_ease_captures.join(" | ")
        );
    }
    if !compiled.info.unsupported_perframe_captures.is_empty() {
        log::debug!(
            "Song lua unsupported perframe captures for '{}': {}",
            song_title,
            compiled.info.unsupported_perframe_captures.join(" | ")
        );
    }

    for (index, overlay) in compiled.overlays.iter().enumerate() {
        let message_names = overlay
            .message_commands
            .iter()
            .map(|command| format!("{}({})", command.message, command.blocks.len()))
            .collect::<Vec<_>>();
        log::debug!(
            "Song lua overlay[{index}] for '{}': kind={:?} name={:?} parent={:?} visible={} xy=({:.1},{:.1}) zoom={:.3}/{:.3}/{:.3} rot=({:.1},{:.1},{:.1}) alpha={:.3} msgs=[{}]",
            song_title,
            overlay.kind,
            overlay.name,
            overlay.parent_index,
            overlay.initial_state.visible,
            overlay.initial_state.x,
            overlay.initial_state.y,
            overlay.initial_state.basezoom,
            overlay.initial_state.zoom_x,
            overlay.initial_state.zoom_y,
            overlay.initial_state.rot_x_deg,
            overlay.initial_state.rot_y_deg,
            overlay.initial_state.rot_z_deg,
            overlay.initial_state.diffuse[3],
            message_names.join(", ")
        );
    }

    for (index, ease) in overlay_eases.iter().enumerate() {
        log::trace!(
            "Song lua overlay_ease[{index}] for '{}': overlay={} start_s={:.3} end_s={:.3} sustain_end_s={:.3} cutoff_s={:?} easing={:?} from={:?} to={:?}",
            song_title,
            ease.overlay_index,
            ease.start_second,
            ease.end_second,
            ease.sustain_end_second,
            ease.cutoff_second,
            ease.easing,
            ease.from,
            ease.to
        );
    }
    for (index, event) in messages.iter().enumerate() {
        log::trace!(
            "Song lua message[{index}] for '{}': beat={:.3} message='{}' persists={}",
            song_title,
            event.beat,
            event.message,
            event.persists
        );
    }
}

const fn song_lua_runtime_summary_is_notable<S: Clone + std::fmt::Debug>(
    compiled: &CompiledSongLua<S>,
    overlay_ease_count: usize,
    total_constant: usize,
    total_eases: usize,
    total_column_offsets: usize,
    unsupported_targets: usize,
) -> bool {
    total_constant > 0
        || total_eases > 0
        || total_column_offsets > 0
        || !compiled.overlays.is_empty()
        || overlay_ease_count > 0
        || !compiled.messages.is_empty()
        || !compiled.sound_paths.is_empty()
        || compiled.info.unsupported_perframes > 0
        || compiled.info.unsupported_function_eases > 0
        || compiled.info.unsupported_function_actions > 0
        || !compiled.info.skipped_message_command_captures.is_empty()
        || unsupported_targets > 0
}

fn log_song_lua_runtime_summary<S: Clone + std::fmt::Debug>(
    song_title: &str,
    compiled: &CompiledSongLua<S>,
    overlay_ease_count: usize,
    total_constant: usize,
    total_eases: usize,
    total_column_offsets: usize,
    unsupported_targets: usize,
    compile_ms: f64,
    runtime_ms: f64,
) {
    log::info!(
        "Compiled gameplay lua for '{}' (constants={}, eases={}, column_offsets={}, overlay_eases={}, overlays={}, messages={}, sound_assets={}, unsupported_targets={}, function_eases={}, function_actions={}, perframes={}, skipped_message_commands={}, compile_ms={compile_ms:.3}, runtime_ms={runtime_ms:.3}).",
        song_title,
        total_constant,
        total_eases,
        total_column_offsets,
        overlay_ease_count,
        compiled.overlays.len(),
        compiled.messages.len(),
        compiled.sound_paths.len(),
        unsupported_targets,
        compiled.info.unsupported_function_eases,
        compiled.info.unsupported_function_actions,
        compiled.info.unsupported_perframes,
        compiled.info.skipped_message_command_captures.len(),
    );
}

fn log_unsupported_song_lua_ease_target(
    player: usize,
    window: &deadsync_gameplay::SongLuaRuntimeEaseWindow,
) {
    if let deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Mod(target_name) = &window.target {
        log::debug!(
            "Unsupported gameplay lua ease target for player {}: target='{}' start={:.3} limit={:.3} span={:?} from={:.3} to={:.3} easing={:?}",
            player + 1,
            target_name,
            window.start,
            window.limit,
            window.span_mode,
            window.from,
            window.to,
            window.easing
        );
    }
}

fn build_song_lua_runtime_windows_for_data<S: Clone + std::fmt::Debug>(
    params: deadsync_gameplay::SongLuaRuntimeWindowBuild<'_>,
    song_lua_data: PreparedGameplaySongLua<S>,
) -> deadsync_gameplay::SongLuaRuntimeBuildOutput<
    SongLuaOverlayActor<S>,
    SongLuaCapturedActor,
    SongLuaRuntimeOverlayStateDelta,
> {
    let mut constant_windows: [Vec<deadsync_gameplay::AttackMaskWindow>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut ease_windows: [Vec<deadsync_gameplay::SongLuaEaseMaskWindow>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut overlays = Vec::new();
    let mut overlay_eases = Vec::new();
    let mut overlay_ease_ranges = Vec::new();
    let mut overlay_events = Vec::new();
    let mut background_visual_layers = Vec::new();
    let mut foreground_visual_layers = Vec::new();
    let mut player_actors: [SongLuaCapturedActor; MAX_PLAYERS] = std::array::from_fn(|player| {
        let default = params.player_actor_defaults[player];
        SongLuaCapturedActor {
            initial_state: SongLuaOverlayState {
                x: default.x,
                y: default.y,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
            manual_hud_draw: false,
            ..SongLuaCapturedActor::default()
        }
    });
    let mut player_events: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut player_judgment_events: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut player_combo_events: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut song_foreground = SongLuaCapturedActor::default();
    let mut song_foreground_events = Vec::new();
    let mut hidden_players = [false; MAX_PLAYERS];
    let mut hidden_screen_layers = [false; 2];
    let mut note_hides: [deadsync_gameplay::SongLuaNoteHideWindows; MAX_PLAYERS] =
        std::array::from_fn(|_| deadsync_gameplay::SongLuaNoteHideWindows::default());
    let mut column_offsets: [Vec<deadsync_gameplay::SongLuaColumnOffsetWindowRuntime>;
        MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());

    if song_lua_data.primary.is_none()
        && song_lua_data.background_layers.is_empty()
        && song_lua_data.foreground_layers.is_empty()
    {
        return (
            constant_windows,
            ease_windows,
            deadsync_gameplay::build_song_lua_runtime_visuals(
                overlays,
                overlay_eases,
                overlay_ease_ranges,
                overlay_events,
                background_visual_layers,
                foreground_visual_layers,
                player_actors,
                player_events,
                player_judgment_events,
                player_combo_events,
                song_foreground,
                song_foreground_events,
                hidden_players,
                hidden_screen_layers,
                note_hides,
                column_offsets,
                params.screen_width,
                params.screen_height,
            ),
        );
    }

    let mut out_screen_width = params.screen_width;
    let mut out_screen_height = params.screen_height;

    if let Some(primary) = song_lua_data.primary.as_ref() {
        let compiled = &primary.compiled;
        let runtime_started = Instant::now();
        overlays = song_lua_runtime_overlays(
            compiled,
            params.timing_players[0],
            params.machine_global_offset_seconds,
        );
        let message_seconds = deadsync_gameplay::build_song_lua_message_seconds(
            compiled.messages.iter().map(|message| message.beat),
            params.timing_players[0],
            params.machine_global_offset_seconds,
        );
        overlay_events = crate::gameplay::build_song_lua_overlay_message_events_with_seconds(
            compiled,
            &message_seconds,
        );
        let overlay_runtime_eases =
            crate::gameplay::build_song_lua_overlay_ease_windows_with_events(
                compiled,
                params.timing_players[0],
                params.machine_global_offset_seconds,
                &overlay_events,
            );
        (overlay_eases, overlay_ease_ranges) = deadsync_gameplay::group_song_lua_overlay_eases(
            compiled.overlays.len(),
            overlay_runtime_eases,
        );
        deadsync_gameplay::apply_song_lua_player_actor_overrides(
            &mut player_actors,
            &compiled.player_actors,
        );
        player_events = deadsync_gameplay::build_song_lua_player_message_events(
            &compiled.player_actors,
            |actor| {
                crate::gameplay::build_song_lua_actor_message_events_for_commands(
                    &compiled.messages,
                    &message_seconds,
                    &actor.message_commands,
                )
            },
        );
        player_judgment_events = deadsync_gameplay::build_song_lua_player_message_events(
            &compiled.player_actors,
            |actor| {
                crate::gameplay::build_song_lua_actor_message_events_for_commands(
                    &compiled.messages,
                    &message_seconds,
                    &actor.judgment.message_commands,
                )
            },
        );
        player_combo_events = deadsync_gameplay::build_song_lua_player_message_events(
            &compiled.player_actors,
            |actor| {
                crate::gameplay::build_song_lua_actor_message_events_for_commands(
                    &compiled.messages,
                    &message_seconds,
                    &actor.combo.message_commands,
                )
            },
        );
        song_foreground = compiled.song_foreground.clone();
        song_foreground_events = crate::gameplay::build_song_lua_actor_message_events_for_commands(
            &compiled.messages,
            &message_seconds,
            &compiled.song_foreground.message_commands,
        );
        hidden_players = deadsync_gameplay::build_song_lua_hidden_players(&compiled.hidden_players);
        hidden_screen_layers = compiled.hidden_screen_layers;
        note_hides = deadsync_gameplay::build_song_lua_note_hide_windows_for_players(
            compiled
                .note_hides
                .iter()
                .map(|hide| (hide.player, hide.column, hide.start_beat, hide.end_beat)),
        );

        for player in 0..params.num_players {
            for column in 0..MAX_COLS {
                if let Some(hide) = compiled
                    .note_hides
                    .iter()
                    .find(|hide| hide.player == player && hide.column == column)
                {
                    note_hides[player].set_zoom_spline(
                        column,
                        hide.spline_beats_per_t,
                        hide.spline_size,
                    );
                }
            }
        }

        let mut unsupported_targets = 0usize;
        let mut total_constant = 0usize;
        let mut total_eases = 0usize;
        let mut total_column_offsets = 0usize;
        let time_mods = song_lua_runtime_mod_windows(&compiled.time_mods);
        let beat_mods = song_lua_runtime_mod_windows(&compiled.beat_mods);
        let eases = song_lua_runtime_ease_windows(&compiled.eases);
        let column_offsets_src = song_lua_runtime_column_offset_windows(&compiled.column_offsets);
        for player in 0..params.num_players {
            let player_global_offset_seconds =
                deadsync_gameplay::effective_player_global_offset_seconds(
                    params.machine_global_offset_seconds,
                    params.player_global_offset_shift_seconds,
                    player,
                );
            let player_windows = deadsync_gameplay::build_song_lua_player_runtime_windows(
                &time_mods,
                &beat_mods,
                &eases,
                &column_offsets_src,
                params.timing_players[player],
                player,
                player_global_offset_seconds,
                |window| log_unsupported_song_lua_ease_target(player, window),
            );
            unsupported_targets += player_windows.unsupported_targets;
            total_constant += player_windows.constant_windows.len();
            total_eases += player_windows.ease_windows.len();
            total_column_offsets += player_windows.column_offsets.len();
            constant_windows[player] = player_windows.constant_windows;
            ease_windows[player] = player_windows.ease_windows;
            column_offsets[player] = player_windows.column_offsets;
        }

        let runtime_ms = runtime_started.elapsed().as_secs_f64() * 1000.0;
        if song_lua_runtime_summary_is_notable(
            compiled,
            overlay_eases.len(),
            total_constant,
            total_eases,
            total_column_offsets,
            unsupported_targets,
        ) {
            log_song_lua_runtime_summary(
                params.song_title,
                compiled,
                overlay_eases.len(),
                total_constant,
                total_eases,
                total_column_offsets,
                unsupported_targets,
                primary.compile_ms,
                runtime_ms,
            );
            log_song_lua_runtime_debug(
                params.song_title,
                compiled,
                &overlay_eases,
                &compiled.messages,
                &hidden_players,
                total_constant,
                total_eases,
                total_column_offsets,
                unsupported_targets,
            );
        }

        out_screen_width = compiled.screen_width;
        out_screen_height = compiled.screen_height;
    }

    for layer_data in &song_lua_data.background_layers {
        let compiled = &layer_data.compiled;
        if let Some(layer) = build_song_lua_compiled_visual_layer_runtime(
            params.song_title,
            layer_data.start_beat,
            compiled,
            params.timing_players[0],
            params.machine_global_offset_seconds,
        ) {
            background_visual_layers.push(layer);
        }
    }

    for layer_data in &song_lua_data.foreground_layers {
        let compiled = &layer_data.compiled;
        if let Some(layer) = build_song_lua_compiled_visual_layer_runtime(
            params.song_title,
            layer_data.start_beat,
            compiled,
            params.timing_players[0],
            params.machine_global_offset_seconds,
        ) {
            foreground_visual_layers.push(layer);
        }
    }

    (
        constant_windows,
        ease_windows,
        deadsync_gameplay::build_song_lua_runtime_visuals(
            overlays,
            overlay_eases,
            overlay_ease_ranges,
            overlay_events,
            background_visual_layers,
            foreground_visual_layers,
            player_actors,
            player_events,
            player_judgment_events,
            player_combo_events,
            song_foreground,
            song_foreground_events,
            hidden_players,
            hidden_screen_layers,
            note_hides,
            column_offsets,
            out_screen_width,
            out_screen_height,
        ),
    )
}

fn song_lua_sound_time_crossed(previous: f32, now: f32, event_second: f32) -> bool {
    if !event_second.is_finite() {
        return false;
    }
    let starts_at_zero = previous <= 0.0 && event_second.abs() <= f32::EPSILON;
    (event_second > previous || starts_at_zero) && event_second <= now
}
pub(crate) fn visit_scheduled_song_lua_sound_events(
    events: &[SongLuaSoundEvent],
    next_event_ix: &mut usize,
    previous: f32,
    now: f32,
    visit: &mut impl FnMut(&Path),
) {
    if !previous.is_finite() || !now.is_finite() {
        return;
    }
    if now < previous {
        // Practice does not dispatch Song-Lua sounds, but gameplay clock
        // correction and future seekable modes still need deterministic replay
        // after moving backwards.
        *next_event_ix = events.partition_point(|event| event.second < now);
        return;
    }
    while let Some(event) = events.get(*next_event_ix) {
        if event.second > now {
            break;
        }
        if song_lua_sound_time_crossed(previous, now, event.second) {
            visit(&event.path);
        }
        *next_event_ix += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn song_lua_sound_crossing_keeps_zero_and_forward_edge_semantics() {
        assert!(song_lua_sound_time_crossed(0.0, 0.0, 0.0));
        assert!(song_lua_sound_time_crossed(1.0, 2.0, 2.0));
        assert!(!song_lua_sound_time_crossed(1.0, 2.0, 1.0));
        assert!(!song_lua_sound_time_crossed(1.0, 2.0, f32::NAN));
    }
    #[test]
    fn song_lua_sound_schedule_advances_without_revisiting_old_events() {
        let events = [
            SongLuaSoundEvent {
                second: 0.0,
                path: PathBuf::from("zero.ogg"),
            },
            SongLuaSoundEvent {
                second: 1.0,
                path: PathBuf::from("one.ogg"),
            },
            SongLuaSoundEvent {
                second: 2.0,
                path: PathBuf::from("two.ogg"),
            },
        ];
        let mut next_event_ix = 0;
        let mut visited = Vec::new();

        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 0.0, 0.0, &mut |path| {
            visited.push(path.to_path_buf());
        });
        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 0.0, 1.5, &mut |path| {
            visited.push(path.to_path_buf());
        });
        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 0.0, 1.5, &mut |path| {
            visited.push(path.to_path_buf());
        });

        assert_eq!(
            visited,
            [PathBuf::from("zero.ogg"), PathBuf::from("one.ogg")]
        );
        assert_eq!(next_event_ix, 2);
    }
    #[test]
    fn song_lua_sound_schedule_rewinds_after_a_backward_seek() {
        let events = [
            SongLuaSoundEvent {
                second: 1.0,
                path: PathBuf::from("one.ogg"),
            },
            SongLuaSoundEvent {
                second: 2.0,
                path: PathBuf::from("two.ogg"),
            },
        ];
        let mut next_event_ix = events.len();
        let mut visited = Vec::new();

        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 2.0, 0.5, &mut |_| {});
        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 0.5, 1.5, &mut |path| {
            visited.push(path.to_path_buf());
        });

        assert_eq!(visited, [PathBuf::from("one.ogg")]);
        assert_eq!(next_event_ix, 1);
    }
}

impl<S> Default for PreparedGameplaySongLua<S> {
    fn default() -> Self {
        Self {
            primary: None,
            background_layers: Vec::new(),
            foreground_layers: Vec::new(),
        }
    }
}
