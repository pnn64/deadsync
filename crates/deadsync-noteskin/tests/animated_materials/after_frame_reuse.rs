// Parent plus exact first-reference reuse, isolating filesystem probes.
use super::baseline::itg_resolve_relative_or_noteskin_path;
use super::*;

pub(super) fn itg_resolve_animated_texture_ini(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<ItgResolvedModelTexture> {
    let ini = noteskin_itg::IniData::parse_file(path).ok()?;
    let (first_frame_idx, frame) = if let Some(frame) = ini.get("AnimatedTexture", "Frame0000") {
        (0, frame)
    } else {
        (1, ini.get("AnimatedTexture", "Frame0001")?)
    };
    let first_frame = frame;
    let texture_path = itg_resolve_relative_or_noteskin_path(data, path, frame)?;
    let tex_velocity_x = ini
        .get("AnimatedTexture", "TexVelocityX")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let tex_velocity_y = ini
        .get("AnimatedTexture", "TexVelocityY")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let tex_offset_x = ini
        .get("AnimatedTexture", "TexOffsetX")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let tex_offset_y = ini
        .get("AnimatedTexture", "TexOffsetY")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let mut cycle_seconds = 0.0f32;
    let mut frames = Vec::new();
    for idx in first_frame_idx..1000 {
        let frame_key = itg_animated_texture_key(*b"Frame0000", idx);
        let delay_key = itg_animated_texture_key(*b"Delay0000", idx);
        let Some(frame) = ini.get("AnimatedTexture", itg_animated_texture_key_str(&frame_key))
        else {
            break;
        };
        let Some(delay) = ini
            .get("AnimatedTexture", itg_animated_texture_key_str(&delay_key))
            .and_then(noteskin_itg::parse_ini_float)
        else {
            break;
        };
        if !delay.is_finite() || delay < 0.0 {
            return None;
        }
        frames.push(ItgTextureFrame {
            // The first image was already resolved before scanning delays.
            // Keep each frame owned without probing that same path again.
            path: if idx == first_frame_idx || frame == first_frame {
                texture_path.clone()
            } else {
                itg_resolve_relative_or_noteskin_path(data, path, frame)?
            },
            delay,
        });
        cycle_seconds += delay;
    }
    Some(ItgResolvedModelTexture {
        sphere_mapped: path.to_string_lossy().contains("sphere"),
        // Repeated references to one image need no atlas. Keep its full UV
        // domain for scrolling materials instead of adding duplicate tiles.
        animation: (frames.iter().any(|frame| frame.path != texture_path)
            && cycle_seconds > f32::EPSILON
            && cycle_seconds.is_finite())
        .then(|| ItgTextureAnimation {
            path: path.to_path_buf(),
            frames,
        }),
        texture_path,
        tex: ItgModelTexturePath {
            uv_velocity: [tex_velocity_x, tex_velocity_y],
            uv_offset: [tex_offset_x, tex_offset_y],
            uv_cycle_seconds: (cycle_seconds > f32::EPSILON && cycle_seconds.is_finite())
                .then_some(cycle_seconds),
        },
    })
}
