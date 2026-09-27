use super::{ItgLuaResolvedSprite, ModelMesh, ModelVertex, SpriteSlot};
use deadsync_noteskin::script::{normalized_script_command, parse_script_bool, split_script_token};
use std::path::Path;
use std::sync::Arc;

#[derive(Default)]
struct DepthMode {
    clear: bool,
    write: bool,
    test: bool,
    invisible: bool,
    nearest: bool,
}

fn depth_mode(sprite: &ItgLuaResolvedSprite) -> DepthMode {
    let mut mode = DepthMode::default();
    for key in ["initcommand", "oncommand"] {
        let Some(script) = sprite.commands.get(key) else {
            continue;
        };
        let script = normalized_script_command(script);
        for raw in script.split(';') {
            let Some(token) = split_script_token(raw) else {
                continue;
            };
            let Some(&arg) = token.args().first() else {
                continue;
            };
            let name = token.command().as_str();
            if name.eq_ignore_ascii_case("clearzbuffer") {
                mode.clear = parse_script_bool(arg);
            } else if name.eq_ignore_ascii_case("zwrite") {
                mode.write = parse_script_bool(arg);
            } else if name.eq_ignore_ascii_case("ztest") {
                mode.test = parse_script_bool(arg);
            } else if name.eq_ignore_ascii_case("settexturefiltering") {
                mode.nearest = !parse_script_bool(arg);
            } else if name.eq_ignore_ascii_case("blend") {
                mode.invisible = arg
                    .trim_matches(['\'', '"'])
                    .eq_ignore_ascii_case("BlendMode_NoEffect");
            }
        }
    }
    mode
}

/// Resolve static, nearest-filtered depth cutouts at the asset boundary. ITG's
/// NoEffect mask writes opaque pixels closer to the camera; the following
/// z-tested sprite can draw only through its transparent pixels. Retained mesh
/// geometry gives every renderer that cutout without shared depth-buffer state
/// leaking between notes. No decoding or tessellation occurs during gameplay.
pub(super) fn apply_sprite_masks(sprites: &mut Vec<ItgLuaResolvedSprite>) {
    let mut mask = None;
    sprites.retain_mut(|sprite| {
        let mode = depth_mode(sprite);
        if mode.clear {
            mask = None;
        }
        if mode.invisible {
            if mode.write {
                let mesh = mode.nearest.then(|| load_mask_mesh(&sprite.slot)).flatten();
                mask = mesh.map(|mesh| {
                    (
                        sprite.slot.def.clone(),
                        sprite.slot.source_size,
                        Arc::new(mesh),
                    )
                });
                if mask.is_none() {
                    log::warn!(
                        "Could not compile static noteskin depth mask '{}'",
                        sprite.slot.texture_key()
                    );
                }
            }
            return false; // BlendMode_NoEffect never writes colour.
        }
        if mode.test
            && let Some((def, size, mesh)) = &mask
        {
            let slot = &mut sprite.slot;
            if slot.model.is_none()
                && slot.source_size == *size
                && slot.def.rotation_deg == def.rotation_deg
                && slot.def.mirror_h == def.mirror_h
                && slot.def.mirror_v == def.mirror_v
            {
                slot.model = Some(Arc::clone(mesh));
                slot.sprite_mesh = true;
            }
        }
        true
    });
}

fn load_mask_mesh(slot: &SpriteSlot) -> Option<ModelMesh> {
    if slot.model.is_some()
        || slot.source.frame_count() != 1
        || slot.custom_uv.is_some()
        || slot.uv_velocity != [0.0; 2]
    {
        return None;
    }
    let path = super::texture::resolve_asset_path(&Path::new("assets").join(slot.texture_key()));
    let image = crate::open_image_fallback(&path).ok()?.into_rgba8();
    let [w, h] = slot
        .def
        .size
        .map(|v| u32::try_from(v).ok())
        .map(|v| v.filter(|&v| v > 0));
    let (w, h) = (w?, h?);
    let [x, y] = slot.def.src.map(|v| u32::try_from(v).ok());
    let (x, y) = (x?, y?);
    if x.checked_add(w)? > image.width()
        || y.checked_add(h)? > image.height()
        || image.width() % w != 0
        || image.height() % h != 0
    {
        return None;
    }
    // Texture translation may select another quant column. A static mesh is
    // valid only when all atlas cells have the same cutout, including alpha=1
    // pixels: ITG discards <=1/256, so only byte alpha=0 leaves the mask open.
    if image
        .enumerate_pixels()
        .any(|(px, py, pixel)| (pixel[3] == 0) != (image.get_pixel(x + px % w, y + py % h)[3] == 0))
    {
        return None;
    }
    Some(cutout_mesh(&image, [x, y, w, h], slot.source_size))
}

fn cutout_mesh(image: &image::RgbaImage, [x, y, w, h]: [u32; 4], size: [i32; 2]) -> ModelMesh {
    let mut rects: Vec<[u32; 4]> = Vec::new();
    for row in 0..h {
        let mut col = 0;
        while col < w {
            if image.get_pixel(x + col, y + row)[3] != 0 {
                col += 1;
                continue;
            }
            let start = col;
            while col < w && image.get_pixel(x + col, y + row)[3] == 0 {
                col += 1;
            }
            if let Some(rect) = rects
                .iter_mut()
                .rev()
                .find(|r| r[0] == start && r[2] == col && r[3] == row)
            {
                rect[3] += 1;
            } else {
                rects.push([start, row, col, row + 1]);
            }
        }
    }
    let [width, height] = size.map(|v| v as f32);
    let mut vertices = Vec::with_capacity(rects.len() * 6);
    for [left, top, right, bottom] in rects {
        for [px, py] in [
            [left, top],
            [left, bottom],
            [right, bottom],
            [left, top],
            [right, bottom],
            [right, top],
        ] {
            let (u, v) = (px as f32 / w as f32, py as f32 / h as f32);
            vertices.push(ModelVertex {
                pos: [(u - 0.5) * width, (0.5 - v) * height, 0.0],
                uv: [u, v],
                tex_matrix_scale: [1.0; 2],
            });
        }
    }
    ModelMesh {
        vertices: vertices.into(),
        // Preserve the full sprite canvas, including its transparent margins.
        bounds: [
            -width * 0.5,
            -height * 0.5,
            0.0,
            width * 0.5,
            height * 0.5,
            0.0,
        ],
    }
}
