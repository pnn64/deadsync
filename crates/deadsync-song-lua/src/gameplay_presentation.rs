//! Compose a game stage with screen and HUD presentation supplied by a presenter.

use crate::gameplay_session::{GameplayStage, StageNoteskins};
use crate::playback::{
    FieldFrame, FieldLayout, FrameOptions, GameplayActorSegments, GameplayCoreState, ScreenLayer,
    compose_frame,
};
use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, FlatDraw};
use deadsync_core::hud_style::NotefieldHudStyle;
use deadsync_gameplay::GameplayProfileData;
use deadsync_gameplay::practice::PracticeSession;
use deadsync_notefield::{
    BuiltNotefield, CapturedActorScratch, NotefieldComposeRequest, NotefieldHudComposeResult,
    NotefieldOptions, PreparedNotefield, TimingSegmentLabel, ViewOverride, prepare_notefield,
};
use deadsync_noteskin::NoteskinSlot;

/// Layout and HUD options shared by normal play and Practice.
#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayFrameView {
    pub notefield: ViewOverride,
    pub hide_gameplay_hud: bool,
}

/// Song-authored content included in a frame.
#[derive(Clone, Copy)]
pub struct SongFrameContent {
    pub apply_attacks: bool,
    pub show_song_visuals: bool,
}

impl SongFrameContent {
    const PLAYBACK: Self = Self {
        apply_attacks: true,
        show_song_visuals: true,
    };
    const PRACTICE_EDITOR: Self = Self {
        apply_attacks: false,
        show_song_visuals: false,
    };
}

#[derive(Clone, Copy)]
pub struct FramePresentationContext<'a, View, Policy> {
    pub assets: &'a AssetManager,
    pub view: View,
    pub uptime_seconds: f64,
    pub policy: Policy,
}

/// Appearance and text resources used when drawing one player's field and HUD.
#[derive(Default)]
pub struct FieldPresentation<'a> {
    pub hud_style: NotefieldHudStyle,
    pub hud_options: NotefieldOptions,
    pub center_one_player: bool,
    pub timing_labels: &'a [TimingSegmentLabel],
    pub edit_measure_text_slot_base: u8,
    pub countdown_font: &'static str,
    pub countdown_text_slot: u8,
}

pub struct FieldHudBuffers<'a> {
    pub actors: &'a mut Vec<Actor>,
    pub draws: &'a mut Vec<FlatDraw>,
    pub capture: &'a mut CapturedActorScratch,
}

/// Screen and HUD drawing for a live stage. Draw hooks read the stage, while
/// frame storage lets the compositor reuse both the stage and theme buffers.
pub trait GameplayFramePresenter {
    type Profile: GameplayProfileData;
    type Slot: NoteskinSlot + Clone;
    type NoteskinAssets: StageNoteskins<Self::Slot>;
    type View: Copy + Into<GameplayFrameView>;
    type Policy: Copy;
    type Scratch;

    fn stage(
        &self,
    ) -> &GameplayStage<GameplayCoreState<Self::Profile, Self::Slot>, Self::NoteskinAssets>;

    fn frame_storage(
        &mut self,
    ) -> (
        &mut GameplayStage<GameplayCoreState<Self::Profile, Self::Slot>, Self::NoteskinAssets>,
        &mut Option<Self::Scratch>,
    );

    fn draw_layers<'a>(
        &'a self,
        scratch: &'a mut Self::Scratch,
        context: FramePresentationContext<'a, Self::View, Self::Policy>,
    ) -> impl FnMut(ScreenLayer, &mut Vec<Actor>, FieldLayout) + 'a;

    fn field_presentation(
        &self,
        context: FramePresentationContext<'_, Self::View, Self::Policy>,
        player: usize,
    ) -> FieldPresentation<'_>;

    #[cfg(feature = "test-support")]
    fn arrow_tipsy_clock_input(&self, _player: usize) -> Option<f64> {
        None
    }

    fn draw_field_hud(
        &self,
        context: FramePresentationContext<'_, Self::View, Self::Policy>,
        frame: &FieldFrame<'_>,
        request: &NotefieldComposeRequest<'_, Self::Slot>,
        prepared: &PreparedNotefield<'_, Self::Slot>,
        buffers: FieldHudBuffers<'_>,
    ) -> NotefieldHudComposeResult;

    fn finish_frame(&self, _actors: &mut Vec<Actor>) {}
}

/// Compose live Gameplay with the chart's attacks and song-authored visuals.
pub fn compose_gameplay_frame<P: GameplayFramePresenter>(
    actors: &mut Vec<Actor>,
    presenter: &mut P,
    assets: &AssetManager,
    view: P::View,
    uptime_seconds: f64,
    policy: P::Policy,
) -> GameplayActorSegments {
    compose_stage_frame(
        actors,
        presenter,
        FramePresentationContext {
            assets,
            view,
            uptime_seconds,
            policy,
        },
        SongFrameContent::PLAYBACK,
    )
}

/// Practice shows the chart's attacks and song-authored visuals during playback.
pub fn compose_practice_frame<P: GameplayFramePresenter>(
    actors: &mut Vec<Actor>,
    presenter: &mut P,
    practice: &PracticeSession,
    assets: &AssetManager,
    view: P::View,
    uptime_seconds: f64,
    policy: P::Policy,
) -> GameplayActorSegments {
    let song_content = if practice.is_playing() {
        SongFrameContent::PLAYBACK
    } else {
        SongFrameContent::PRACTICE_EDITOR
    };
    compose_stage_frame(
        actors,
        presenter,
        FramePresentationContext {
            assets,
            view,
            uptime_seconds,
            policy,
        },
        song_content,
    )
}

/// Render an explicit combination for frame comparison tests.
#[cfg(feature = "test-support")]
pub fn compose_gameplay_preview<P: GameplayFramePresenter>(
    actors: &mut Vec<Actor>,
    presenter: &mut P,
    context: FramePresentationContext<'_, P::View, P::Policy>,
    song_content: SongFrameContent,
) -> GameplayActorSegments {
    compose_stage_frame(actors, presenter, context, song_content)
}

/// Borrow the stage and presentation buffers for one frame and restore both
/// before final overlays.
fn compose_stage_frame<P: GameplayFramePresenter>(
    actors: &mut Vec<Actor>,
    presenter: &mut P,
    context: FramePresentationContext<'_, P::View, P::Policy>,
    song_content: SongFrameContent,
) -> GameplayActorSegments {
    let (stage, presentation) = presenter.frame_storage();
    let mut song = stage
        .song_scratch
        .take()
        .expect("song scratch restored after every frame");
    let mut presentation = presentation
        .take()
        .expect("presentation scratch restored after every frame");

    let stage = presenter.stage();
    let session = &stage.gameplay.setup.session;
    let frame_view = context.view.into();
    let options = FrameOptions {
        uptime_seconds: context.uptime_seconds,
        play_style: session.play_style,
        player_side: session.player_side,
        hide_song_bg: stage
            .gameplay
            .profiles()
            .each_ref()
            .map(GameplayProfileData::hide_song_background),
        notefield: frame_view.notefield,
        hide_gameplay_hud: frame_view.hide_gameplay_hud,
        apply_attacks: song_content.apply_attacks,
        show_song_visuals: song_content.show_song_visuals,
    };
    let textures = context.assets.texture_context();
    let actor_resources = stage.actor_resources();
    let segments = compose_frame(
        actors,
        &stage.gameplay,
        &stage.song_media,
        &mut song,
        context.assets,
        options,
        presenter.draw_layers(&mut presentation, context),
        |frame, cameras, field, field_flat, hud, hud_flat| {
            field.clear();
            field_flat.clear();
            hud.clear();
            hud_flat.clear();

            let player = frame.player;
            let style = presenter.field_presentation(context, player);
            let mut geometry = frame.geometry;
            geometry.center_one_player |= style.center_one_player;
            let request = NotefieldComposeRequest {
                timing_labels: style.timing_labels,
                hud_style: style.hud_style,
                placement: frame.placement,
                view: frame.view,
                geometry,
                visual: frame.visual,
                chart: frame.chart,
                noteskin: stage.notefield_noteskins(player),
                song_lua: frame.song_lua,
                settings: stage.notefield_settings(player),
                options: style.hud_options,
                capture_requests: frame.capture,
                edit_measure_text_slot_base: style.edit_measure_text_slot_base,
                arrow_effect_time_s: context.uptime_seconds,
                #[cfg(feature = "test-support")]
                arrow_tipsy_time_s: presenter.arrow_tipsy_clock_input(player),
                #[cfg(not(feature = "test-support"))]
                arrow_tipsy_time_s: None,
                music_time_s: frame.music_time_s,
            };
            let Some(prepared) = prepare_notefield(&request) else {
                return BuiltNotefield::empty(request.geometry.screen_center_x);
            };
            let mut scratch = stage.notefield_scratch(player);
            let sprite_source = |slot: &P::Slot| {
                stage
                    .noteskin_assets
                    .actor_texture_source(slot, actor_resources, textures)
            };
            hud.reserve(prepared.frame_plan.hud_actor_reserve);
            let field_result = scratch.compose(
                &stage.gameplay,
                field,
                field_flat,
                hud_flat,
                &mut cameras[player],
                &request,
                &prepared,
                style.countdown_font,
                style.countdown_text_slot,
                &sprite_source,
            );
            let hud_result = presenter.draw_field_hud(
                context,
                frame,
                &request,
                &prepared,
                FieldHudBuffers {
                    actors: hud,
                    draws: hud_flat,
                    capture: scratch.hud_capture_scratch(),
                },
            );
            BuiltNotefield {
                layout_center_x: prepared.field.layout_center_x,
                field_camera: field_result.camera,
                field_camera_generation: field_result.camera_generation,
                field_actors: field_result.captured_actors,
                field_draw_range: field_result.draw_range,
                judgment_actors: hud_result.judgment_actors,
                judgment_draw_range: hud_result.judgment_draw_range,
                combo_actors: hud_result.combo_actors,
                combo_draw_range: hud_result.combo_draw_range,
                hud_parts: hud_result.parts,
            }
        },
    );
    let (stage, scratch) = presenter.frame_storage();
    stage.song_scratch = Some(song);
    *scratch = Some(presentation);
    presenter.finish_frame(actors);
    segments
}
