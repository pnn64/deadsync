//! Compose a game stage with screen and notefield actors supplied by a presenter.

use crate::gameplay_session::GameplayStage;
use crate::playback::{
    FieldFrame, FieldLayout, FrameOptions, GameplayActorSegments, GameplayCoreState, ScreenLayer,
    compose_frame,
};
use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, FlatDraw};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::GameplayProfileData;
use deadsync_gameplay::practice::PracticeSession;
use deadsync_notefield::{BuiltNotefield, NotefieldCameraCache, ViewOverride};
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

pub struct FieldRenderBuffers<'a> {
    pub cameras: &'a mut [NotefieldCameraCache; MAX_PLAYERS],
    pub field: &'a mut Vec<Actor>,
    pub field_flat: &'a mut Vec<FlatDraw>,
    pub hud: &'a mut Vec<Actor>,
    pub hud_flat: &'a mut Vec<FlatDraw>,
}

/// Screen and field drawing for a live stage. Draw hooks read the stage, while
/// frame storage lets the compositor reuse both the stage and theme buffers.
pub trait GameplayFramePresenter {
    type Profile: GameplayProfileData;
    type Slot: NoteskinSlot + Clone;
    type View: Copy + Into<GameplayFrameView>;
    type Policy: Copy;
    type Scratch;

    fn stage(&self) -> &GameplayStage<GameplayCoreState<Self::Profile, Self::Slot>>;

    fn frame_storage(
        &mut self,
    ) -> (
        &mut GameplayStage<GameplayCoreState<Self::Profile, Self::Slot>>,
        &mut Option<Self::Scratch>,
    );

    fn draw_layers<'a>(
        &'a self,
        scratch: &'a mut Self::Scratch,
        context: FramePresentationContext<'a, Self::View, Self::Policy>,
    ) -> impl FnMut(ScreenLayer, &mut Vec<Actor>, FieldLayout) + 'a;

    fn draw_field(
        &self,
        context: FramePresentationContext<'_, Self::View, Self::Policy>,
        request: FieldFrame,
        buffers: FieldRenderBuffers<'_>,
    ) -> BuiltNotefield;

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
    let segments = compose_frame(
        actors,
        &stage.gameplay,
        &stage.song_media,
        &mut song,
        context.assets,
        options,
        presenter.draw_layers(&mut presentation, context),
        |request, cameras, field, field_flat, hud, hud_flat| {
            presenter.draw_field(
                context,
                request,
                FieldRenderBuffers {
                    cameras,
                    field,
                    field_flat,
                    hud,
                    hud_flat,
                },
            )
        },
    );
    let (stage, scratch) = presenter.frame_storage();
    stage.song_scratch = Some(song);
    *scratch = Some(presentation);
    presenter.finish_frame(actors);
    segments
}
