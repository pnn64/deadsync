use crate::game::parsing::noteskin::SpriteSlot;
use deadlib_present::actors::TextAttribute;
use deadlib_render::TexturedMeshVertex;

pub use deadsync_song_lua::{
    SongLuaOverlayActor as GenericSongLuaOverlayActor, SongLuaOverlayBlendMode,
    SongLuaOverlayCommandBlock, SongLuaOverlayEase, SongLuaOverlayMeshVertex,
    SongLuaOverlayMessageCommand, SongLuaOverlayModelDraw,
    SongLuaOverlayModelLayer as GenericSongLuaOverlayModelLayer, SongLuaOverlayState,
    SongLuaOverlayStateDelta, SongLuaProxyTarget, SongLuaTextGlowMode, overlay_delta_from_blocks,
    overlay_state_after_blocks, parse_overlay_blend_mode, parse_overlay_effect_clock,
    parse_overlay_effect_mode, parse_overlay_text_align, parse_overlay_text_glow_mode,
};

pub type SongLuaOverlayModelLayer = GenericSongLuaOverlayModelLayer<TexturedMeshVertex>;
pub type SongLuaOverlayKind =
    deadsync_song_lua::SongLuaOverlayKind<SpriteSlot, TexturedMeshVertex, TextAttribute>;

pub type SongLuaOverlayActor = GenericSongLuaOverlayActor<SongLuaOverlayKind>;

#[cfg(test)]
mod tests {
    use super::{
        SongLuaOverlayBlendMode, parse_overlay_blend_mode, parse_overlay_effect_clock,
        parse_overlay_effect_mode,
    };
    use deadlib_present::anim::{EffectClock, EffectMode};

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
