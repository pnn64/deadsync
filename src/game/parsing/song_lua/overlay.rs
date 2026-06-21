use crate::game::parsing::noteskin::SpriteSlot;
use deadlib_present::actors::TextAttribute;
use deadlib_render::TexturedMeshVertex;
use std::path::PathBuf;
use std::sync::Arc;

pub use deadsync_song_lua::{
    overlay_delta_from_blocks, overlay_delta_intersection, overlay_state_after_blocks,
    parse_overlay_blend_mode, parse_overlay_effect_clock, parse_overlay_effect_mode,
    parse_overlay_text_align, parse_overlay_text_glow_mode, SongLuaOverlayBlendMode,
    SongLuaOverlayActor as GenericSongLuaOverlayActor, SongLuaOverlayCommandBlock,
    SongLuaOverlayEase, SongLuaOverlayMeshVertex,
    SongLuaOverlayMessageCommand, SongLuaOverlayModelDraw,
    SongLuaOverlayModelLayer as GenericSongLuaOverlayModelLayer, SongLuaOverlayState,
    SongLuaOverlayStateDelta, SongLuaProxyTarget, SongLuaTextGlowMode,
};

#[derive(Debug, Clone)]
pub enum SongLuaOverlayKind {
    Actor,
    ActorFrame,
    ActorFrameTexture,
    ActorProxy {
        target: SongLuaProxyTarget,
    },
    AftSprite {
        capture_name: String,
    },
    Sprite {
        texture_path: PathBuf,
        texture_key: Arc<str>,
    },
    Sound {
        sound_path: PathBuf,
    },
    BitmapText {
        font_name: &'static str,
        font_path: PathBuf,
        text: Arc<str>,
        stroke_color: Option<[f32; 4]>,
        attributes: Arc<[TextAttribute]>,
    },
    ActorMultiVertex {
        vertices: Arc<[SongLuaOverlayMeshVertex]>,
        texture_path: Option<PathBuf>,
        texture_key: Option<Arc<str>>,
    },
    Model {
        layers: Arc<[SongLuaOverlayModelLayer]>,
    },
    NoteskinActor {
        slots: Arc<[SpriteSlot]>,
    },
    SongMeterDisplay {
        stream_width: f32,
        stream_state: SongLuaOverlayState,
        music_length_seconds: f32,
    },
    GraphDisplay {
        size: [f32; 2],
        body_values: Arc<[f32]>,
        body_state: SongLuaOverlayState,
        line_state: SongLuaOverlayState,
    },
    Quad,
}

pub type SongLuaOverlayModelLayer = GenericSongLuaOverlayModelLayer<TexturedMeshVertex>;

pub type SongLuaOverlayActor = GenericSongLuaOverlayActor<SongLuaOverlayKind>;



#[cfg(test)]
mod tests {
    use super::{
        SongLuaOverlayBlendMode, parse_overlay_blend_mode, parse_overlay_effect_clock,
        parse_overlay_effect_mode,
    };
    
    #[test]
    fn parse_overlay_blend_mode_accepts_stepmania_add_name() {
        assert_eq!(
            parse_overlay_blend_mode("BlendMode_Add"),
            Some(SongLuaOverlayBlendMode::Add)
        );
        assert_eq!(
            parse_overlay_blend_mode("BlendMode_Multiply"),
            Some(SongLuaOverlayBlendMode::Multiply)
        );
        assert_eq!(
            parse_overlay_blend_mode("BlendMode_Subtract"),
            Some(SongLuaOverlayBlendMode::Subtract)
        );
    }

    #[test]
    fn parse_overlay_effect_mode_accepts_song_lua_effect_names() {
        assert_eq!(
            parse_overlay_effect_mode("DiffuseRamp"),
            Some(EffectMode::DiffuseRamp)
        );
        assert_eq!(
            parse_overlay_effect_mode("glowshift"),
            Some(EffectMode::GlowShift)
        );
        assert_eq!(
            parse_overlay_effect_mode("bounce"),
            Some(EffectMode::Bounce)
        );
        assert_eq!(parse_overlay_effect_mode("wag"), Some(EffectMode::Wag));
    }

    #[test]
    fn parse_overlay_effect_clock_accepts_music_and_bgm_aliases() {
        assert_eq!(parse_overlay_effect_clock("beat"), Some(EffectClock::Beat));
        assert_eq!(parse_overlay_effect_clock("bgm"), Some(EffectClock::Beat));
        assert_eq!(parse_overlay_effect_clock("music"), Some(EffectClock::Time));
    }
}



