use super::{ItgLuaResolvedSprite, ModelMesh, ModelVertex, SpriteSlot};
use deadsync_noteskin::script::{
    ScriptCommand, normalized_script_command, parse_script_bool, split_script_token,
};
use std::path::Path;
use std::sync::{Arc, LazyLock};

static EMPTY_MASK_VERTICES: LazyLock<Arc<[ModelVertex]>> = LazyLock::new(|| Arc::from([]));

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
            match token.command() {
                ScriptCommand::ClearZBuffer => mode.clear = parse_script_bool(arg),
                ScriptCommand::ZWrite => mode.write = parse_script_bool(arg),
                ScriptCommand::ZTest => mode.test = parse_script_bool(arg),
                ScriptCommand::SetTextureFiltering => mode.nearest = !parse_script_bool(arg),
                ScriptCommand::Blend => {
                    mode.invisible = arg
                        .trim_matches(['\'', '"'])
                        .eq_ignore_ascii_case("BlendMode_NoEffect");
                }
                _ => {}
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
    if !mask_cells_match(&image, [x, y, w, h]) {
        return None;
    }
    Some(cutout_mesh(&image, [x, y, w, h], slot.source_size))
}

fn mask_cells_match(image: &image::RgbaImage, [x, y, w, h]: [u32; 4]) -> bool {
    if image.width() == 0 || image.height() == 0 {
        return true;
    }
    let bytes = image.as_raw();
    let stride = image.width() as usize * 4;
    // Preserve cheap rejection before constructing the row/cell iterators.
    let first_reference = y as usize * stride + x as usize * 4 + 3;
    if (bytes[3] == 0) != (bytes[first_reference] == 0) {
        return false;
    }
    let cell_bytes = w as usize * 4;
    for (row, pixels) in bytes
        .chunks_exact(stride)
        .take(image.height() as usize)
        .enumerate()
    {
        let origin = (y as usize + row % h as usize) * stride + x as usize * 4;
        let reference = &bytes[origin..origin + cell_bytes];
        let reference_pixels = reference.as_chunks::<4>().0;
        for cell in pixels.chunks_exact(cell_bytes) {
            // The selected cell always has its own alpha pattern.
            if cell.as_ptr() == reference.as_ptr() {
                continue;
            }
            if cell
                .as_chunks::<4>()
                .0
                .iter()
                .zip(reference_pixels)
                .any(|(pixel, reference)| (pixel[3] == 0) != (reference[3] == 0))
            {
                return false;
            }
        }
    }
    true
}

// The two row frontiers normally fit on the stack. Only unusually fragmented
// rows spill into one reusable buffer; interleaved slots retain both rows.
struct MaskRows {
    inline: [[usize; 64]; 2],
    lengths: [usize; 2],
    spill: Vec<usize>,
    initial_spill: usize,
}

impl MaskRows {
    fn new(width: u32) -> Self {
        Self {
            inline: [[0; 64]; 2],
            lengths: [0; 2],
            spill: Vec::new(),
            initial_spill: (width as usize).div_ceil(2).saturating_sub(64).min(256) * 2,
        }
    }

    fn get(&self, side: usize, index: usize) -> Option<usize> {
        if index >= self.lengths[side] {
            None
        } else if index < 64 {
            Some(self.inline[side][index])
        } else {
            Some(self.spill[(index - 64) * 2 + side])
        }
    }

    fn push(&mut self, side: usize, value: usize) {
        let index = self.lengths[side];
        if index < 64 {
            self.inline[side][index] = value;
        } else {
            let offset = (index - 64) * 2 + side;
            if self.spill.is_empty() {
                self.spill.reserve_exact(self.initial_spill);
            }
            if offset >= self.spill.len() {
                self.spill.resize(offset + 1, 0);
            }
            self.spill[offset] = value;
        }
        self.lengths[side] += 1;
    }
}

fn mask_rectangles(image: &image::RgbaImage, [x, y, w, h]: [u32; 4]) -> Vec<[u32; 4]> {
    let mut rects: Vec<[u32; 4]> = Vec::new();
    let mut active = MaskRows::new(w);
    for row in 0..h {
        let side = (row & 1) as usize;
        let previous = 1 - side;
        active.lengths[side] = 0;
        let mut cursor = 0;
        let origin = ((y as usize + row as usize) * image.width() as usize + x as usize) * 4;
        let pixels = &image.as_raw()[origin..origin + w as usize * 4];
        let mut col = 0;
        while col < w {
            if pixels[col as usize * 4 + 3] != 0 {
                col += 1;
                continue;
            }
            let start = col;
            while col < w && pixels[col as usize * 4 + 3] == 0 {
                col += 1;
            }
            // Previous-row runs are sorted by column. Advance once through
            // that frontier instead of searching all accumulated rectangles.
            while let Some(index) = active.get(previous, cursor) {
                if rects[index][0] >= start {
                    break;
                }
                cursor += 1;
            }
            let index = if let Some(index) = active.get(previous, cursor)
                && rects[index][0] == start
                && rects[index][2] == col
            {
                rects[index][3] += 1;
                cursor += 1;
                index
            } else {
                let index = rects.len();
                rects.push([start, row, col, row + 1]);
                index
            };
            active.push(side, index);
        }
    }
    rects
}

fn mesh_from_rectangles(rects: Vec<[u32; 4]>, [w, h]: [u32; 2], size: [i32; 2]) -> ModelMesh {
    let [width, height] = size.map(|v| v as f32);
    // Each rectangle contributes exactly six vertices. Collecting this known-
    // length iterator lets Arc allocate the output directly, without a Vec.
    let vertices = if rects.is_empty() {
        Arc::clone(&EMPTY_MASK_VERTICES)
    } else {
        rects
            .into_iter()
            .flat_map(|[left, top, right, bottom]| {
                [
                    [left, top],
                    [left, bottom],
                    [right, bottom],
                    [left, top],
                    [right, bottom],
                    [right, top],
                ]
                .map(|[px, py]| {
                    let (u, v) = (px as f32 / w as f32, py as f32 / h as f32);
                    ModelVertex {
                        normal: [0.0, 0.0, 1.0],
                        pos: [(u - 0.5) * width, (0.5 - v) * height, 0.0],
                        uv: [u, v],
                        tex_matrix_scale: [1.0; 2],
                    }
                })
            })
            .collect()
    };
    ModelMesh {
        vertices,
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

fn cutout_mesh(image: &image::RgbaImage, [x, y, w, h]: [u32; 4], size: [i32; 2]) -> ModelMesh {
    mesh_from_rectangles(mask_rectangles(image, [x, y, w, h]), [w, h], size)
}
