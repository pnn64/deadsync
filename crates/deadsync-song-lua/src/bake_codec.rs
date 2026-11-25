//! Persisted form of a completed bake.
//!
//! Most baked types derive their codec. The types here need more care: a
//! retained Lua scene cannot leave its process, reference-audit captures have
//! no persisted form, and leaked font names are rebuilt from the font path
//! they were derived from. Encoding fails rather than drop any of them.
use crate::{CompiledSongLua, SongLuaOverlayKind};
use bincode::de::{BorrowDecode, BorrowDecoder, Decoder};
use bincode::enc::Encoder;
use bincode::error::{DecodeError, EncodeError};
use bincode::{Decode, Encode};
use std::path::PathBuf;

impl<A: Encode> Encode for CompiledSongLua<A> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        // Naming every field makes a new one a compile error here instead of
        // a value that silently reverts to its default on a cache hit.
        let Self {
            live_scene,
            live_indices,
            foreground_beats,
            loaded_steps,
            loaded_style,
            loaded_players,
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
            overlay_capture_calibration_seconds,
            overlay_updates,
            #[cfg(feature = "test-support")]
            overlay_writes,
            #[cfg(feature = "test-support")]
            boolean_writes,
            #[cfg(feature = "test-support")]
            speed_writes,
            #[cfg(feature = "test-support")]
            state_calls,
            #[cfg(feature = "test-support")]
            noteskin_writes,
            stateful_message_captures,
            player_actors,
            song_foreground,
            screen_layers,
            hidden_players,
            note_hides,
            column_offsets,
            column_splines,
            column_spline_origin,
            info,
        } = self;
        if live_scene.is_some() {
            return Err(EncodeError::Other(
                "a bake that retains its Lua scene cannot be persisted",
            ));
        }
        // Reference-audit captures have no persisted form. Dropping them
        // would make a cache hit differ from the bake it replaces.
        #[cfg(feature = "test-support")]
        if !(overlay_writes.is_empty()
            && boolean_writes.is_empty()
            && speed_writes.is_empty()
            && state_calls.is_empty()
            && noteskin_writes.is_empty())
        {
            return Err(EncodeError::Other(
                "reference-audit captures cannot be persisted",
            ));
        }
        live_indices.encode(encoder)?;
        loaded_steps.encode(encoder)?;
        loaded_style.encode(encoder)?;
        loaded_players.encode(encoder)?;
        startup_reloads.encode(encoder)?;
        startup.encode(encoder)?;
        entry_path.encode(encoder)?;
        screen_width.encode(encoder)?;
        screen_height.encode(encoder)?;
        beat_mods.encode(encoder)?;
        time_mods.encode(encoder)?;
        eases.encode(encoder)?;
        messages.encode(encoder)?;
        sound_paths.encode(encoder)?;
        overlays.encode(encoder)?;
        draw_frames.encode(encoder)?;
        screen_overlay_index.encode(encoder)?;
        overlay_eases.encode(encoder)?;
        overlay_update_unit.encode(encoder)?;
        overlay_capture_calibration_seconds.encode(encoder)?;
        overlay_updates.encode(encoder)?;
        stateful_message_captures.encode(encoder)?;
        player_actors.encode(encoder)?;
        song_foreground.encode(encoder)?;
        screen_layers.encode(encoder)?;
        hidden_players.encode(encoder)?;
        note_hides.encode(encoder)?;
        column_offsets.encode(encoder)?;
        column_splines.encode(encoder)?;
        column_spline_origin.encode(encoder)?;
        foreground_beats
            .as_ref()
            .map(|beats| (*beats.start(), *beats.end()))
            .encode(encoder)?;
        info.encode(encoder)
    }
}

macro_rules! decode_compiled {
    ($decoder:ident, $decode:path) => {{
        // Field order is the wire order written by `encode`.
        Ok(Self {
            live_scene: None,
            live_indices: $decode($decoder)?,
            loaded_steps: $decode($decoder)?,
            loaded_style: $decode($decoder)?,
            loaded_players: $decode($decoder)?,
            startup_reloads: $decode($decoder)?,
            startup: $decode($decoder)?,
            entry_path: $decode($decoder)?,
            screen_width: $decode($decoder)?,
            screen_height: $decode($decoder)?,
            beat_mods: $decode($decoder)?,
            time_mods: $decode($decoder)?,
            eases: $decode($decoder)?,
            messages: $decode($decoder)?,
            sound_paths: $decode($decoder)?,
            overlays: $decode($decoder)?,
            draw_frames: $decode($decoder)?,
            screen_overlay_index: $decode($decoder)?,
            overlay_eases: $decode($decoder)?,
            overlay_update_unit: $decode($decoder)?,
            overlay_capture_calibration_seconds: $decode($decoder)?,
            overlay_updates: $decode($decoder)?,
            // `encode` refuses a bake that holds reference-audit captures.
            #[cfg(feature = "test-support")]
            overlay_writes: Vec::new(),
            #[cfg(feature = "test-support")]
            boolean_writes: Vec::new(),
            #[cfg(feature = "test-support")]
            speed_writes: Vec::new(),
            #[cfg(feature = "test-support")]
            state_calls: Vec::new(),
            #[cfg(feature = "test-support")]
            noteskin_writes: Vec::new(),
            stateful_message_captures: $decode($decoder)?,
            player_actors: $decode($decoder)?,
            song_foreground: $decode($decoder)?,
            screen_layers: $decode($decoder)?,
            hidden_players: $decode($decoder)?,
            note_hides: $decode($decoder)?,
            column_offsets: $decode($decoder)?,
            column_splines: $decode($decoder)?,
            column_spline_origin: $decode($decoder)?,
            foreground_beats: {
                let beats: Option<(f32, f32)> = $decode($decoder)?;
                beats.map(|(start, end)| start..=end)
            },
            info: $decode($decoder)?,
        })
    }};
}

impl<Context, A: Decode<Context>> Decode<Context> for CompiledSongLua<A> {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decode_compiled!(decoder, Decode::decode)
    }
}

impl<'de, Context, A: BorrowDecode<'de, Context>> BorrowDecode<'de, Context>
    for CompiledSongLua<A>
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        decode_compiled!(decoder, BorrowDecode::borrow_decode)
    }
}

/// Font names are leaked once per process; restore the same static for a
/// persisted name instead of leaking one string per cache hit.
fn persisted_font_name(name: &str, font_path: &std::path::Path) -> Option<&'static str> {
    match name {
        "wendy" => Some("wendy"),
        "miso" => Some("miso"),
        _ => {
            let restored = crate::bitmap_font::song_lua_font_name(font_path);
            (restored == name).then_some(restored)
        }
    }
}

const KIND_ACTOR: u8 = 0;
const KIND_ACTOR_FRAME: u8 = 1;
const KIND_WRAPPER_STATE: u8 = 2;
const KIND_UPDATE_TRACKS: u8 = 3;
const KIND_ACTOR_FRAME_TEXTURE: u8 = 4;
const KIND_ACTOR_PROXY: u8 = 5;
const KIND_AFT_SPRITE: u8 = 6;
const KIND_SPRITE: u8 = 7;
const KIND_SOUND: u8 = 8;
const KIND_BITMAP_TEXT: u8 = 9;
const KIND_ACTOR_MULTI_VERTEX: u8 = 10;
const KIND_MODEL: u8 = 11;
const KIND_NOTESKIN_ACTOR: u8 = 12;
const KIND_SONG_METER_DISPLAY: u8 = 13;
const KIND_GRAPH_DISPLAY: u8 = 14;
const KIND_QUAD: u8 = 15;

impl<S: Encode, V: Encode, T: Encode> Encode for SongLuaOverlayKind<S, V, T> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        match self {
            Self::Actor => KIND_ACTOR.encode(encoder),
            Self::ActorFrame => KIND_ACTOR_FRAME.encode(encoder),
            Self::WrapperState => KIND_WRAPPER_STATE.encode(encoder),
            Self::UpdateTracks {
                tracks,
                draw_frames,
            } => {
                KIND_UPDATE_TRACKS.encode(encoder)?;
                tracks.encode(encoder)?;
                draw_frames.encode(encoder)
            }
            Self::ActorFrameTexture {
                capture_name,
                alpha_buffer,
                depth_buffer,
                float_buffer,
                capture_size,
            } => {
                KIND_ACTOR_FRAME_TEXTURE.encode(encoder)?;
                capture_name.encode(encoder)?;
                alpha_buffer.encode(encoder)?;
                depth_buffer.encode(encoder)?;
                float_buffer.encode(encoder)?;
                capture_size.encode(encoder)
            }
            Self::ActorProxy { target } => {
                KIND_ACTOR_PROXY.encode(encoder)?;
                target.encode(encoder)
            }
            Self::AftSprite { capture_name } => {
                KIND_AFT_SPRITE.encode(encoder)?;
                capture_name.encode(encoder)
            }
            Self::Sprite {
                texture_path,
                texture_key,
                states,
                textures,
            } => {
                KIND_SPRITE.encode(encoder)?;
                texture_path.encode(encoder)?;
                texture_key.encode(encoder)?;
                states.encode(encoder)?;
                textures.encode(encoder)
            }
            Self::Sound { sound_path } => {
                KIND_SOUND.encode(encoder)?;
                sound_path.encode(encoder)
            }
            Self::BitmapText {
                font_name,
                font_path,
                text,
                text_changes,
                text_changes_in_seconds,
                stroke_color,
                attributes,
            } => {
                if persisted_font_name(font_name, font_path).is_none() {
                    return Err(EncodeError::Other(
                        "a BitmapText font name is not derived from its font path",
                    ));
                }
                KIND_BITMAP_TEXT.encode(encoder)?;
                font_name.encode(encoder)?;
                font_path.encode(encoder)?;
                text.encode(encoder)?;
                text_changes.encode(encoder)?;
                text_changes_in_seconds.encode(encoder)?;
                stroke_color.encode(encoder)?;
                attributes.encode(encoder)
            }
            Self::ActorMultiVertex {
                vertices,
                texture_path,
                texture_key,
                capture_name,
            } => {
                KIND_ACTOR_MULTI_VERTEX.encode(encoder)?;
                vertices.encode(encoder)?;
                texture_path.encode(encoder)?;
                texture_key.encode(encoder)?;
                capture_name.encode(encoder)
            }
            Self::Model { layers } => {
                KIND_MODEL.encode(encoder)?;
                layers.encode(encoder)
            }
            Self::NoteskinActor {
                slots,
                texture_samples,
            } => {
                KIND_NOTESKIN_ACTOR.encode(encoder)?;
                slots.encode(encoder)?;
                texture_samples.encode(encoder)
            }
            Self::SongMeterDisplay {
                stream_width,
                stream_state,
                music_length_seconds,
            } => {
                KIND_SONG_METER_DISPLAY.encode(encoder)?;
                stream_width.encode(encoder)?;
                stream_state.encode(encoder)?;
                music_length_seconds.encode(encoder)
            }
            Self::GraphDisplay {
                size,
                body_values,
                body_state,
                line_state,
            } => {
                KIND_GRAPH_DISPLAY.encode(encoder)?;
                size.encode(encoder)?;
                body_values.encode(encoder)?;
                body_state.encode(encoder)?;
                line_state.encode(encoder)
            }
            Self::Quad => KIND_QUAD.encode(encoder),
        }
    }
}

macro_rules! decode_kind {
    ($decoder:ident, $decode:path) => {{
        Ok(match u8::decode($decoder)? {
            KIND_ACTOR => Self::Actor,
            KIND_ACTOR_FRAME => Self::ActorFrame,
            KIND_WRAPPER_STATE => Self::WrapperState,
            KIND_UPDATE_TRACKS => Self::UpdateTracks {
                tracks: $decode($decoder)?,
                draw_frames: $decode($decoder)?,
            },
            KIND_ACTOR_FRAME_TEXTURE => Self::ActorFrameTexture {
                capture_name: $decode($decoder)?,
                alpha_buffer: $decode($decoder)?,
                depth_buffer: $decode($decoder)?,
                float_buffer: $decode($decoder)?,
                capture_size: $decode($decoder)?,
            },
            KIND_ACTOR_PROXY => Self::ActorProxy {
                target: $decode($decoder)?,
            },
            KIND_AFT_SPRITE => Self::AftSprite {
                capture_name: $decode($decoder)?,
            },
            KIND_SPRITE => Self::Sprite {
                texture_path: $decode($decoder)?,
                texture_key: $decode($decoder)?,
                states: $decode($decoder)?,
                textures: $decode($decoder)?,
            },
            KIND_SOUND => Self::Sound {
                sound_path: $decode($decoder)?,
            },
            KIND_BITMAP_TEXT => {
                let name = String::decode($decoder)?;
                let font_path = PathBuf::decode($decoder)?;
                let font_name = persisted_font_name(&name, &font_path).ok_or(
                    DecodeError::Other("a BitmapText font name does not match its font path"),
                )?;
                Self::BitmapText {
                    font_name,
                    font_path,
                    text: $decode($decoder)?,
                    text_changes: $decode($decoder)?,
                    text_changes_in_seconds: $decode($decoder)?,
                    stroke_color: $decode($decoder)?,
                    attributes: $decode($decoder)?,
                }
            }
            KIND_ACTOR_MULTI_VERTEX => Self::ActorMultiVertex {
                vertices: $decode($decoder)?,
                texture_path: $decode($decoder)?,
                texture_key: $decode($decoder)?,
                capture_name: $decode($decoder)?,
            },
            KIND_MODEL => Self::Model {
                layers: $decode($decoder)?,
            },
            KIND_NOTESKIN_ACTOR => Self::NoteskinActor {
                slots: $decode($decoder)?,
                texture_samples: $decode($decoder)?,
            },
            KIND_SONG_METER_DISPLAY => Self::SongMeterDisplay {
                stream_width: $decode($decoder)?,
                stream_state: $decode($decoder)?,
                music_length_seconds: $decode($decoder)?,
            },
            KIND_GRAPH_DISPLAY => Self::GraphDisplay {
                size: $decode($decoder)?,
                body_values: $decode($decoder)?,
                body_state: $decode($decoder)?,
                line_state: $decode($decoder)?,
            },
            KIND_QUAD => Self::Quad,
            _ => return Err(DecodeError::Other("unknown overlay kind")),
        })
    }};
}

impl<Context, S, V, T> Decode<Context> for SongLuaOverlayKind<S, V, T>
where
    S: Decode<Context> + 'static,
    V: Decode<Context> + 'static,
    T: Decode<Context> + 'static,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decode_kind!(decoder, Decode::decode)
    }
}

impl<'de, Context, S, V, T> BorrowDecode<'de, Context> for SongLuaOverlayKind<S, V, T>
where
    S: BorrowDecode<'de, Context> + 'de,
    V: BorrowDecode<'de, Context> + 'de,
    T: BorrowDecode<'de, Context> + 'de,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        decode_kind!(decoder, BorrowDecode::borrow_decode)
    }
}

/// Compile-time proof that a whole preparation is persistable for any
/// persistable noteskin slot type.
#[allow(dead_code)]
fn assert_preparation_is_persistable<S: Encode + Decode<()> + 'static>() {
    fn persistable<T: Encode + Decode<()>>() {}
    persistable::<crate::playback::PreparedGameplaySongLua<S>>();
}
