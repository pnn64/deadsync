use crate::assets;
use crate::core::gfx as renderer;
use crate::core::gfx::{BlendMode, RenderList, RenderObject};
use crate::core::space::Metrics;
use crate::ui::actors::{self, Actor, SizeSpec};
use crate::ui::{anim, font};
use cgmath::{Matrix4, Rad, Vector2, Vector3};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

/* ======================= RENDERER SCREEN BUILDER ======================= */

#[inline(always)]
pub fn build_screen<'a>(
    actors: &'a [actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &'a HashMap<&'static str, font::Font>,
    total_elapsed: f32,
) -> RenderList<'a> {
    let mut text_cache = TextLayoutCache::default();
    build_screen_cached(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        &mut text_cache,
    )
}

#[inline(always)]
pub fn build_screen_cached<'a>(
    actors: &'a [actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &'a HashMap<&'static str, font::Font>,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
) -> RenderList<'a> {
    let mut objects = Vec::with_capacity(estimate_object_count(actors));
    let mut cameras: Vec<Matrix4<f32>> = Vec::with_capacity(4);
    cameras.push(cgmath::ortho(m.left, m.right, m.bottom, m.top, -1.0, 1.0));
    let mut order_counter: u32 = 0;
    let mut masks: Vec<WorldRect> = Vec::with_capacity(8);

    let root_rect = SmRect {
        x: 0.0,
        y: 0.0,
        w: m.right - m.left,
        h: m.top - m.bottom,
    };
    let parent_z: i16 = 0;
    let camera: u8 = 0;

    for actor in actors {
        build_actor_recursive(
            actor,
            root_rect,
            m,
            fonts,
            parent_z,
            camera,
            &mut cameras,
            &mut masks,
            &mut order_counter,
            &mut objects,
            text_cache,
            total_elapsed,
        );
    }

    // `order` is already monotonically assigned, so we do not need a stable sort here.
    objects.sort_unstable_by_key(|o| (o.z, o.order));

    RenderList {
        clear_color,
        cameras,
        objects,
    }
}

#[derive(Clone, Copy)]
struct CachedGlyph {
    texture_key: *const str,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    size: [f32; 2],
    offset: [f32; 2],
    advance_i32: i32,
    draw_quad: bool,
}

#[derive(Clone)]
struct CachedLine {
    width_i32: i32,
    glyphs: Vec<CachedGlyph>,
}

#[derive(Clone)]
struct CachedTextLayout {
    max_logical_width_i: i32,
    glyph_count: usize,
    lines: Vec<CachedLine>,
}

struct OwnedLayoutEntry {
    layout: Box<CachedTextLayout>,
    last_used: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TextLayoutKey {
    font_key: u64,
    wrap_width_pixels: i32,
}

pub struct TextLayoutCache {
    owned_entries: HashMap<TextLayoutKey, HashMap<Box<str>, OwnedLayoutEntry>>,
    shared_aliases: HashMap<TextLayoutKey, HashMap<usize, (Arc<str>, *const CachedTextLayout)>>,
    entry_count: usize,
    alias_count: usize,
    max_entries: usize,
    max_aliases: usize,
    use_tick: u64,
}

impl Default for TextLayoutCache {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl TextLayoutCache {
    pub fn new(max_entries: usize) -> Self {
        let max_entries = max_entries.max(1);
        Self {
            owned_entries: HashMap::new(),
            shared_aliases: HashMap::new(),
            entry_count: 0,
            alias_count: 0,
            max_entries,
            max_aliases: max_entries.saturating_mul(8),
            use_tick: 0,
        }
    }

    pub fn clear(&mut self) {
        self.owned_entries.clear();
        self.shared_aliases.clear();
        self.entry_count = 0;
        self.alias_count = 0;
        self.use_tick = 0;
    }

    #[inline(always)]
    fn next_use_tick(&mut self) -> u64 {
        self.use_tick = self.use_tick.saturating_add(1);
        self.use_tick
    }

    fn prune_owned_entries(&mut self) {
        if self.entry_count < self.max_entries {
            return;
        }
        let keep = self
            .max_entries
            .saturating_sub((self.max_entries / 4).max(1));
        let remove = self.entry_count.saturating_sub(keep).max(1);
        let mut ages = Vec::with_capacity(self.entry_count);
        for font_entries in self.owned_entries.values() {
            ages.extend(font_entries.values().map(|entry| entry.last_used));
        }
        if ages.is_empty() {
            self.clear();
            return;
        }
        let cutoff_ix = remove.saturating_sub(1).min(ages.len().saturating_sub(1));
        ages.select_nth_unstable(cutoff_ix);
        let cutoff = ages[cutoff_ix];
        let mut removed = 0usize;
        self.owned_entries.retain(|_, font_entries| {
            font_entries.retain(|_, entry| {
                let drop = removed < remove && entry.last_used <= cutoff;
                removed += usize::from(drop);
                !drop
            });
            !font_entries.is_empty()
        });
        if removed == 0 {
            self.clear();
            return;
        }
        self.entry_count = self.entry_count.saturating_sub(removed);
        self.shared_aliases.clear();
        self.alias_count = 0;
    }

    #[inline(always)]
    fn touch_owned_layout(
        &mut self,
        key: TextLayoutKey,
        text: &str,
        tick: u64,
    ) -> Option<*const CachedTextLayout> {
        let entry = self.owned_entries.get_mut(&key)?.get_mut(text)?;
        entry.last_used = tick;
        Some(entry.layout.as_ref() as *const CachedTextLayout)
    }

    fn insert_owned_layout(
        &mut self,
        key: TextLayoutKey,
        text: &str,
        layout: CachedTextLayout,
        tick: u64,
    ) -> *const CachedTextLayout {
        if self.entry_count >= self.max_entries {
            // Avoid hard-clearing the entire cache; that was causing visible
            // compose spikes once gameplay churn hit the entry cap.
            self.prune_owned_entries();
        }
        self.owned_entries.entry(key).or_default().insert(
            text.into(),
            OwnedLayoutEntry {
                layout: Box::new(layout),
                last_used: tick,
            },
        );
        self.entry_count += 1;
        self.touch_owned_layout(key, text, tick)
            .expect("owned text layout cache entry inserted")
    }

    fn get_or_build(
        &mut self,
        font: &font::Font,
        fonts: &HashMap<&'static str, font::Font>,
        content: &actors::TextContent,
        wrap_width_pixels: Option<i32>,
    ) -> &CachedTextLayout {
        let key = TextLayoutKey {
            font_key: font_chain_key(font, fonts),
            wrap_width_pixels: wrap_width_pixels.unwrap_or(-1),
        };
        match content {
            actors::TextContent::Owned(text) => self.get_or_build_owned(key, font, fonts, text),
            actors::TextContent::Shared(text) => self.get_or_build_shared(key, font, fonts, text),
        }
    }

    fn get_or_build_owned(
        &mut self,
        key: TextLayoutKey,
        font: &font::Font,
        fonts: &HashMap<&'static str, font::Font>,
        text: &str,
    ) -> &CachedTextLayout {
        let tick = self.next_use_tick();
        if let Some(layout_ptr) = self.touch_owned_layout(key, text, tick) {
            return unsafe { &*layout_ptr };
        }
        let layout = build_cached_text_layout(font, fonts, text, key.wrap_width_pixels);
        let layout_ptr = self.insert_owned_layout(key, text, layout, tick);
        unsafe { &*layout_ptr }
    }

    fn get_or_build_shared(
        &mut self,
        key: TextLayoutKey,
        font: &font::Font,
        fonts: &HashMap<&'static str, font::Font>,
        text: &Arc<str>,
    ) -> &CachedTextLayout {
        let tick = self.next_use_tick();
        let text_key = Arc::as_ptr(text) as *const () as usize;
        if self
            .shared_aliases
            .get(&key)
            .and_then(|font_entries| font_entries.get(&text_key))
            .is_some()
        {
            if let Some(layout_ptr) = self.touch_owned_layout(key, text.as_ref(), tick) {
                return unsafe { &*layout_ptr };
            }
            self.shared_aliases
                .entry(key)
                .or_default()
                .remove(&text_key);
            self.alias_count = self.alias_count.saturating_sub(1);
        }
        let layout_ptr = if let Some(layout_ptr) = self.touch_owned_layout(key, text.as_ref(), tick)
        {
            layout_ptr
        } else {
            let layout = build_cached_text_layout(font, fonts, text, key.wrap_width_pixels);
            self.insert_owned_layout(key, text, layout, tick)
        };
        if self.alias_count >= self.max_aliases {
            self.shared_aliases.clear();
            self.alias_count = 0;
        }
        if self
            .shared_aliases
            .entry(key)
            .or_default()
            .insert(text_key, (text.clone(), layout_ptr))
            .is_none()
        {
            self.alias_count += 1;
        }
        unsafe { &*layout_ptr }
    }
}

fn font_chain_key(font: &font::Font, fonts: &HashMap<&'static str, font::Font>) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut current = Some(font);
    while let Some(font) = current {
        (font as *const font::Font as usize).hash(&mut hasher);
        current = font.fallback_font_name.and_then(|name| fonts.get(name));
    }
    hasher.finish()
}

fn build_cached_text_layout(
    font: &font::Font,
    fonts: &HashMap<&'static str, font::Font>,
    text: &str,
    wrap_width_pixels: i32,
) -> CachedTextLayout {
    let draws_space = font.glyph_map.contains_key(&' ');
    let mut max_logical_width_i = 0i32;
    let mut glyph_count = 0usize;
    let mut lines = Vec::new();

    let mut push_line = |line: &str| {
        let mut width_i32 = 0i32;
        let mut glyphs = Vec::with_capacity(line.chars().count());
        for ch in line.chars() {
            let Some(glyph) = font::find_glyph(font, ch, fonts) else {
                continue;
            };
            width_i32 += glyph.advance_i32;
            glyphs.push(CachedGlyph {
                texture_key: std::ptr::from_ref(glyph.texture_key.as_str()),
                uv_scale: glyph.uv_scale,
                uv_offset: glyph.uv_offset,
                size: glyph.size,
                offset: glyph.offset,
                advance_i32: glyph.advance_i32,
                draw_quad: ch != ' ' || draws_space,
            });
        }
        max_logical_width_i = max_logical_width_i.max(width_i32);
        glyph_count += glyphs.len();
        lines.push(CachedLine { width_i32, glyphs });
    };

    for line in wrapped_text_lines(text, wrap_width_pixels, font, fonts) {
        push_line(line.as_ref());
    }

    CachedTextLayout {
        max_logical_width_i,
        glyph_count,
        lines,
    }
}

fn wrap_text_lines_by_words<F>(
    text: &str,
    wrap_width_pixels: i32,
    space_width: i32,
    mut word_width: F,
) -> Vec<Box<str>>
where
    F: FnMut(&str) -> i32,
{
    let mut out = Vec::new();
    for src in text.split('\n') {
        if wrap_width_pixels < 0 {
            out.push(src.into());
            continue;
        }
        let mut words = src.split(' ').filter(|word| !word.is_empty());
        let Some(first) = words.next() else {
            out.push("".into());
            continue;
        };
        let mut line = String::from(first);
        let mut line_width = word_width(first);
        for word in words {
            let width_to_add = space_width + word_width(word);
            if line_width + width_to_add <= wrap_width_pixels {
                line.push(' ');
                line.push_str(word);
                line_width += width_to_add;
            } else {
                out.push(line.into_boxed_str());
                line = word.to_owned();
                line_width = word_width(word);
            }
        }
        out.push(line.into_boxed_str());
    }
    out
}

fn wrapped_text_lines(
    text: &str,
    wrap_width_pixels: i32,
    font: &font::Font,
    fonts: &HashMap<&'static str, font::Font>,
) -> Vec<Box<str>> {
    let space_width = font::measure_line_width_logical(font, " ", fonts);
    wrap_text_lines_by_words(text, wrap_width_pixels, space_width, |word| {
        font::measure_line_width_logical(font, word, fonts)
    })
}

#[inline(always)]
unsafe fn str_from_cached_ptr<'a>(ptr: *const str) -> &'a str {
    unsafe { &*ptr }
}

#[inline(always)]
fn estimate_object_count(actors: &[Actor]) -> usize {
    #[inline(always)]
    fn count_actor(actor: &Actor) -> usize {
        match actor {
            Actor::Sprite { visible, .. } => usize::from(*visible),
            Actor::Text { content, .. } => content.len() * 2,
            Actor::Mesh {
                visible, vertices, ..
            } => usize::from(*visible && !vertices.is_empty()),
            Actor::TexturedMesh {
                visible, vertices, ..
            } => usize::from(*visible && !vertices.is_empty()),
            Actor::Frame {
                children,
                background,
                ..
            } => children
                .iter()
                .fold(usize::from(background.is_some()), |sum, child| {
                    sum.saturating_add(count_actor(child))
                }),
            Actor::Camera { children, .. } => children
                .iter()
                .fold(0usize, |sum, child| sum.saturating_add(count_actor(child))),
            Actor::Shadow { child, .. } => count_actor(child),
        }
    }

    actors
        .iter()
        .fold(0usize, |sum, actor| sum.saturating_add(count_actor(actor)))
}

/* ======================= ACTOR -> OBJECT CONVERSION ======================= */

#[derive(Clone, Copy)]
struct SmRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[inline(always)]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn apply_effect_to_sprite(
    effect: anim::EffectState,
    elapsed: f32,
    tint: &mut [f32; 4],
    scale: &mut [f32; 2],
    rot_deg: &mut [f32; 3],
) {
    // We currently don't have song beat/time split plumbed here, so use elapsed for both.
    let beat = elapsed;
    if matches!(effect.mode, anim::EffectMode::Spin) {
        // ITGmania spin uses effect delta from clock and does not use effectoffset.
        let units = anim::effect_clock_units(effect, elapsed, beat);
        rot_deg[0] = (rot_deg[0] + effect.magnitude[0] * units).rem_euclid(360.0);
        rot_deg[1] = (rot_deg[1] + effect.magnitude[1] * units).rem_euclid(360.0);
        rot_deg[2] = (rot_deg[2] + effect.magnitude[2] * units).rem_euclid(360.0);
    }

    if let Some(percent) = anim::effect_mix(effect, elapsed, beat) {
        match effect.mode {
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in tint.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], percent).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::DiffuseShift => {
                let between = (((percent + 0.25) * 2.0 * std::f32::consts::PI).sin() * 0.5 + 0.5)
                    .clamp(0.0, 1.0);
                for (i, out) in tint.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], between).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                let offset = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom = lerp_f32(effect.magnitude[0], effect.magnitude[1], offset).max(0.0);
                let sx = lerp_f32(effect.color2[0], effect.color1[0], offset).max(0.0);
                let sy = lerp_f32(effect.color2[1], effect.color1[1], offset).max(0.0);
                scale[0] *= zoom * sx;
                scale[1] *= zoom * sy;
            }
            anim::EffectMode::GlowShift | anim::EffectMode::Spin | anim::EffectMode::None => {}
        }
    }

    tint[0] = tint[0].clamp(0.0, 1.0);
    tint[1] = tint[1].clamp(0.0, 1.0);
    tint[2] = tint[2].clamp(0.0, 1.0);
    tint[3] = tint[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}

#[inline(always)]
fn apply_effect_to_text(
    effect: anim::EffectState,
    elapsed: f32,
    color: &mut [f32; 4],
    scale: &mut [f32; 2],
) {
    // We currently don't have song beat/time split plumbed here, so use elapsed for both.
    let beat = elapsed;
    if let Some(percent) = anim::effect_mix(effect, elapsed, beat) {
        match effect.mode {
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in color.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], percent).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::DiffuseShift => {
                let between = (((percent + 0.25) * 2.0 * std::f32::consts::PI).sin() * 0.5 + 0.5)
                    .clamp(0.0, 1.0);
                for (i, out) in color.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], between).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                let offset = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom = lerp_f32(effect.magnitude[0], effect.magnitude[1], offset).max(0.0);
                let sx = lerp_f32(effect.color2[0], effect.color1[0], offset).max(0.0);
                let sy = lerp_f32(effect.color2[1], effect.color1[1], offset).max(0.0);
                scale[0] *= zoom * sx;
                scale[1] *= zoom * sy;
            }
            anim::EffectMode::GlowShift | anim::EffectMode::Spin | anim::EffectMode::None => {}
        }
    }

    color[0] = color[0].clamp(0.0, 1.0);
    color[1] = color[1].clamp(0.0, 1.0);
    color[2] = color[2].clamp(0.0, 1.0);
    color[3] = color[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}

#[inline(always)]
fn build_actor_recursive<'a>(
    actor: &'a actors::Actor,
    parent: SmRect,
    m: &Metrics,
    fonts: &'a HashMap<&'static str, font::Font>,
    base_z: i16,
    camera: u8,
    cameras: &mut Vec<Matrix4<f32>>,
    masks: &mut Vec<WorldRect>,
    order_counter: &mut u32,
    out: &mut Vec<RenderObject<'a>>,
    text_cache: &mut TextLayoutCache,
    total_elapsed: f32,
) {
    match actor {
        actors::Actor::Sprite {
            align,
            offset,
            size,
            source,
            tint,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            blend,
            mask_source,
            mask_dest,
            glow: _,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            rot_z_deg,
            rot_x_deg,
            rot_y_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            effect,
        } => {
            if !*visible {
                return;
            }

            let (is_solid, texture_name) = match source {
                actors::SpriteSource::Solid => (true, "__white"),
                actors::SpriteSource::Texture(name) => (false, name.as_ref()),
            };

            let mut chosen_cell = *cell;
            let mut chosen_grid = *grid;

            if !is_solid && uv_rect.is_none() {
                let (cols, rows) = grid.unwrap_or_else(|| assets::sprite_sheet_dims(texture_name));
                let total = cols.saturating_mul(rows).max(1);

                let start_linear: u32 = match *cell {
                    Some((cx, cy)) if cy != u32::MAX => {
                        let cx = cx.min(cols.saturating_sub(1));
                        let cy = cy.min(rows.saturating_sub(1));
                        cy.saturating_mul(cols).saturating_add(cx)
                    }
                    Some((i, _)) => i,
                    None => 0,
                };

                if *animate && *state_delay > 0.0 && total > 1 {
                    let steps = (total_elapsed / *state_delay).floor().max(0.0) as u32;
                    let idx = (start_linear + (steps % total)) % total;
                    chosen_cell = Some((idx, u32::MAX));
                    chosen_grid = Some((cols, rows));
                } else if chosen_cell.is_none() && total > 1 {
                    chosen_cell = Some((0, u32::MAX));
                    chosen_grid = Some((cols, rows));
                }
            }

            let mut effect_tint = *tint;
            let mut effect_scale = *scale;
            let mut effect_rot = [*rot_x_deg, *rot_y_deg, *rot_z_deg];
            apply_effect_to_sprite(
                *effect,
                total_elapsed,
                &mut effect_tint,
                &mut effect_scale,
                &mut effect_rot,
            );

            let resolved_size = resolve_sprite_size_like_sm(
                *size,
                is_solid,
                texture_name,
                *uv_rect,
                chosen_cell,
                chosen_grid,
                effect_scale,
            );

            let rect = place_rect(parent, *align, *offset, resolved_size);
            let mask_rect = sm_rect_to_world_edges(rect, m);
            if *mask_source {
                masks.push(mask_rect);
            }
            if *mask_source && !*mask_dest {
                return;
            }
            if *mask_dest && masks.is_empty() {
                return;
            }

            let before = out.len();
            push_sprite(
                out,
                camera,
                rect,
                m,
                is_solid,
                texture_name,
                effect_tint,
                *uv_rect,
                chosen_cell,
                chosen_grid,
                *flip_x,
                *flip_y,
                *cropleft,
                *cropright,
                *croptop,
                *cropbottom,
                *fadeleft,
                *faderight,
                *fadetop,
                *fadebottom,
                *blend,
                effect_rot[0],
                effect_rot[1],
                effect_rot[2],
                *local_offset,
                *local_offset_rot_sin_cos,
                *texcoordvelocity,
                total_elapsed,
            );
            if *mask_dest {
                clip_objects_range_to_world_masks(out, before, masks);
            }

            let layer = base_z.saturating_add(*z);
            for obj in out.iter_mut().skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
        }

        actors::Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            mode,
            visible,
            blend,
            z,
        } => {
            if !*visible || vertices.is_empty() {
                return;
            }

            let rect = place_rect(parent, *align, *offset, *size);
            let base_x = m.left + rect.x;
            let base_y = m.top - rect.y;
            let transform = Matrix4::from_translation(Vector3::new(base_x, base_y, 0.0))
                * Matrix4::from_nonuniform_scale(1.0, -1.0, 1.0);

            let before = out.len();
            out.push(renderer::RenderObject {
                object_type: renderer::ObjectType::Mesh {
                    vertices: std::borrow::Cow::Borrowed(vertices.as_ref()),
                    mode: *mode,
                },
                texture_handle: renderer::INVALID_TEXTURE_HANDLE,
                transform,
                blend: *blend,
                z: 0,
                order: 0,
                camera,
            });

            let layer = base_z.saturating_add(*z);
            for obj in out.iter_mut().skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
        }

        actors::Actor::TexturedMesh {
            align,
            offset,
            size,
            texture,
            vertices,
            mode,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            visible,
            blend,
            z,
        } => {
            if !*visible || vertices.is_empty() {
                return;
            }

            let rect = place_rect(parent, *align, *offset, *size);
            let base_x = m.left + rect.x;
            let base_y = m.top - rect.y;
            let transform = Matrix4::from_translation(Vector3::new(base_x, base_y, 0.0))
                * Matrix4::from_nonuniform_scale(1.0, -1.0, 1.0);

            let before = out.len();
            out.push(renderer::RenderObject {
                object_type: renderer::ObjectType::TexturedMesh {
                    texture_id: std::borrow::Cow::Borrowed(texture.as_ref()),
                    vertices: std::borrow::Cow::Borrowed(vertices.as_ref()),
                    mode: *mode,
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    uv_tex_shift: *uv_tex_shift,
                },
                texture_handle: renderer::INVALID_TEXTURE_HANDLE,
                transform,
                blend: *blend,
                z: 0,
                order: 0,
                camera,
            });

            let layer = base_z.saturating_add(*z);
            for obj in out.iter_mut().skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
        }

        actors::Actor::Shadow { len, color, child } => {
            // Build the child first to push its objects; then duplicate those objects
            // with a pre-multiplied world translation and shadow tint at z-1.
            let start = out.len();
            build_actor_recursive(
                child,
                parent,
                m,
                fonts,
                base_z,
                camera,
                cameras,
                masks,
                order_counter,
                out,
                text_cache,
                total_elapsed,
            );

            // Prepare world-space translation matrix that matches StepMania's
            // DISPLAY->TranslateWorld behavior.
            let t_world = Matrix4::from_translation(Vector3::new(len[0], len[1], 0.0));

            // Duplicate each object produced for the child as a shadow pass.
            let end = out.len();
            for i in start..end {
                let obj = &out[i];
                let mut obj_type = obj.object_type.clone();
                match &mut obj_type {
                    renderer::ObjectType::Sprite { tint, .. } => {
                        // Multiply alpha like SM: shadow.a *= child_alpha
                        let mut shadow_tint = *color;
                        shadow_tint[3] *= (*tint)[3];
                        *tint = shadow_tint;
                    }
                    renderer::ObjectType::Mesh { vertices, .. } => {
                        let sc = *color;
                        let mut out = Vec::with_capacity(vertices.len());
                        for v in vertices.iter() {
                            out.push(renderer::MeshVertex {
                                pos: v.pos,
                                color: [
                                    v.color[0] * sc[0],
                                    v.color[1] * sc[1],
                                    v.color[2] * sc[2],
                                    v.color[3] * sc[3],
                                ],
                            });
                        }
                        *vertices = std::borrow::Cow::Owned(out);
                    }
                    renderer::ObjectType::TexturedMesh { vertices, .. } => {
                        let sc = *color;
                        let mut out = Vec::with_capacity(vertices.len());
                        for v in vertices.iter() {
                            out.push(renderer::TexturedMeshVertex {
                                pos: v.pos,
                                uv: v.uv,
                                tex_matrix_scale: v.tex_matrix_scale,
                                color: [
                                    v.color[0] * sc[0],
                                    v.color[1] * sc[1],
                                    v.color[2] * sc[2],
                                    v.color[3] * sc[3],
                                ],
                            });
                        }
                        *vertices = std::borrow::Cow::Owned(out);
                    }
                }

                out.push(renderer::RenderObject {
                    object_type: obj_type,
                    texture_handle: renderer::INVALID_TEXTURE_HANDLE,
                    transform: t_world * obj.transform,
                    blend: obj.blend,
                    // Draw behind the original to ensure correct order without
                    // having to rewind the global order counter.
                    z: obj.z.saturating_sub(1),
                    order: obj.order, // order doesn't matter since z is lower
                    camera: obj.camera,
                });
            }
        }

        actors::Actor::Camera {
            view_proj,
            children,
        } => {
            cameras.push(*view_proj);
            let id = cameras.len().saturating_sub(1).try_into().unwrap_or(0u8);
            for child in children {
                build_actor_recursive(
                    child,
                    parent,
                    m,
                    fonts,
                    base_z,
                    id,
                    cameras,
                    masks,
                    order_counter,
                    out,
                    text_cache,
                    total_elapsed,
                );
            }
        }

        actors::Actor::Text {
            align,
            offset,
            color,
            stroke_color,
            font,
            content,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            wrap_width_pixels,
            max_width,
            max_height,
            // NEW:
            max_w_pre_zoom,
            max_h_pre_zoom,
            clip,
            blend,
            glow: _,
            effect,
        } => {
            if let Some(fm) = fonts.get(font) {
                let mut effect_color = *color;
                let mut effect_scale = *scale;
                apply_effect_to_text(*effect, total_elapsed, &mut effect_color, &mut effect_scale);
                let before = out.len();
                layout_text(
                    out,
                    fm,
                    fonts,
                    content,
                    0.0, // _px_size unused
                    effect_scale,
                    *fit_width,
                    *fit_height,
                    *wrap_width_pixels,
                    *max_width,
                    *max_height,
                    // NEW flags:
                    *max_w_pre_zoom,
                    *max_h_pre_zoom,
                    parent,
                    *align,
                    *offset,
                    *align_text,
                    m,
                    text_cache,
                );
                if let Some([x, y, w, h]) = *clip {
                    let clip_sm = SmRect {
                        x: parent.x + x,
                        y: parent.y + y,
                        w,
                        h,
                    };
                    let clip_world = sm_rect_to_world_edges(clip_sm, m);
                    clip_objects_range_to_world_rect(out, before, clip_world);
                }
                let end = out.len();
                let layer = base_z.saturating_add(*z);
                let mut stroke_rgba = stroke_color.unwrap_or(fm.default_stroke_color);
                stroke_rgba[3] *= effect_color[3];
                if stroke_rgba[3] > 0.0 && !fm.stroke_texture_map.is_empty() {
                    out.reserve(end - before);
                    let mut cached_texture_ptr: *const u8 = std::ptr::null();
                    let mut cached_texture_len = 0usize;
                    let mut cached_stroke: Option<&str> = None;
                    let mut idx = before;
                    while idx < end {
                        let (
                            stroke_key,
                            transform,
                            uv_scale,
                            uv_offset,
                            local_offset,
                            local_offset_rot_sin_cos,
                            edge_fade,
                        ) = match &out[idx] {
                            RenderObject {
                                object_type:
                                    renderer::ObjectType::Sprite {
                                        texture_id,
                                        uv_scale,
                                        uv_offset,
                                        local_offset,
                                        local_offset_rot_sin_cos,
                                        edge_fade,
                                        ..
                                    },
                                transform,
                                ..
                            } => {
                                let texture_key = texture_id.as_ref();
                                let texture_bytes = texture_key.as_bytes();
                                let stroke_key = if texture_bytes.len() == cached_texture_len
                                    && !cached_texture_ptr.is_null()
                                    && {
                                        // Glyph texture keys are borrowed from font storage, so this
                                        // cached byte slice stays valid across pushes after reserve().
                                        let cached_bytes = unsafe {
                                            std::slice::from_raw_parts(
                                                cached_texture_ptr,
                                                cached_texture_len,
                                            )
                                        };
                                        cached_bytes == texture_bytes
                                    } {
                                    cached_stroke
                                } else {
                                    cached_texture_ptr = texture_bytes.as_ptr();
                                    cached_texture_len = texture_bytes.len();
                                    cached_stroke = fm
                                        .stroke_texture_map
                                        .get(texture_key)
                                        .map(std::string::String::as_str);
                                    cached_stroke
                                };
                                let Some(stroke_key) = stroke_key else {
                                    idx += 1;
                                    continue;
                                };
                                (
                                    stroke_key,
                                    *transform,
                                    *uv_scale,
                                    *uv_offset,
                                    *local_offset,
                                    *local_offset_rot_sin_cos,
                                    *edge_fade,
                                )
                            }
                            _ => {
                                idx += 1;
                                continue;
                            }
                        };
                        out.push(RenderObject {
                            object_type: renderer::ObjectType::Sprite {
                                texture_id: std::borrow::Cow::Borrowed(stroke_key),
                                tint: stroke_rgba,
                                uv_scale,
                                uv_offset,
                                local_offset,
                                local_offset_rot_sin_cos,
                                edge_fade,
                            },
                            texture_handle: renderer::INVALID_TEXTURE_HANDLE,
                            transform,
                            blend: *blend,
                            z: layer,
                            order: {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            },
                            camera,
                        });
                        idx += 1;
                    }
                }
                for obj in out.iter_mut().take(end).skip(before) {
                    obj.z = layer;
                    obj.order = {
                        let o = *order_counter;
                        *order_counter += 1;
                        o
                    };
                    obj.blend = *blend;
                    obj.camera = camera;
                    if let renderer::ObjectType::Sprite { tint, .. } = &mut obj.object_type {
                        *tint = effect_color;
                    }
                }
            }
        }

        actors::Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        } => {
            let rect = place_rect(parent, *align, *offset, *size);
            let layer = base_z.saturating_add(*z);

            if let Some(bg) = background {
                match bg {
                    actors::Background::Color(c) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            camera,
                            rect,
                            m,
                            true,
                            "__white",
                            *c,
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            0.0,
                            0.0,
                            [0.0, 0.0],
                            [0.0, 1.0],
                            None,
                            total_elapsed,
                        );
                        for obj in out.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                    actors::Background::Texture(tex) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            camera,
                            rect,
                            m,
                            false,
                            tex,
                            [1.0; 4],
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            0.0,
                            0.0,
                            [0.0, 0.0],
                            [0.0, 1.0],
                            None,
                            total_elapsed,
                        );
                        for obj in out.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                }
            }

            for child in children {
                build_actor_recursive(
                    child,
                    rect,
                    m,
                    fonts,
                    layer,
                    camera,
                    cameras,
                    masks,
                    order_counter,
                    out,
                    text_cache,
                    total_elapsed,
                );
            }
        }
    }
}

/* ======================= LAYOUT HELPERS ======================= */

#[inline(always)]
fn resolve_sprite_size_like_sm(
    size: [SizeSpec; 2],
    is_solid: bool,
    texture_name: &str,
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    scale: [f32; 2],
) -> [SizeSpec; 2] {
    use SizeSpec::Px;

    #[inline(always)]
    fn native_dims(
        is_solid: bool,
        texture_name: &str,
        uv: Option<[f32; 4]>,
        cell: Option<(u32, u32)>,
        grid: Option<(u32, u32)>,
    ) -> (f32, f32) {
        if is_solid {
            return (1.0, 1.0);
        }
        let Some(meta) = assets::texture_dims(texture_name) else {
            return (0.0, 0.0);
        };
        let (mut tw, mut th) = (meta.w as f32, meta.h as f32);
        if let Some([u0, v0, u1, v1]) = uv {
            tw *= (u1 - u0).abs().max(1e-6);
            th *= (v1 - v0).abs().max(1e-6);
        } else if cell.is_some() {
            let (gc, gr) = grid.unwrap_or_else(|| assets::sprite_sheet_dims(texture_name));
            let cols = gc.max(1);
            let rows = gr.max(1);
            tw /= cols as f32;
            th /= rows as f32;
        }
        (tw, th)
    }

    let (nw, nh) = native_dims(is_solid, texture_name, uv_rect, cell, grid);
    let aspect = if nw > 0.0 && nh > 0.0 { nh / nw } else { 1.0 };

    match (size[0], size[1]) {
        (Px(w), Px(h)) if w == 0.0 && h == 0.0 => [Px(nw * scale[0]), Px(nh * scale[1])],
        (Px(w), Px(h)) if w > 0.0 && h == 0.0 => [Px(w), Px(w * aspect)],
        (Px(w), Px(h)) if w == 0.0 && h > 0.0 => {
            let inv_aspect = if aspect > 0.0 { 1.0 / aspect } else { 1.0 };
            [Px(h * inv_aspect), Px(h)]
        }
        _ => size,
    }
}

#[inline(always)]
fn place_rect(parent: SmRect, align: [f32; 2], offset: [f32; 2], size: [SizeSpec; 2]) -> SmRect {
    let w = match size[0] {
        SizeSpec::Px(w) => w,
        SizeSpec::Fill => parent.w,
    };
    let h = match size[1] {
        SizeSpec::Px(h) => h,
        SizeSpec::Fill => parent.h,
    };
    let rx = parent.x;
    let ry = parent.y;
    let ax = align[0];
    let ay = align[1];
    SmRect {
        x: ax.mul_add(-w, rx + offset[0]),
        y: ay.mul_add(-h, ry + offset[1]),
        w,
        h,
    }
}

#[inline(always)]
fn calculate_uvs(
    texture: &str,
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    flip_x: bool,
    flip_y: bool,
    cl: f32,
    cr: f32,
    ct: f32,
    cb: f32,
    texcoordvelocity: Option<[f32; 2]>,
    total_elapsed: f32,
) -> ([f32; 2], [f32; 2]) {
    let (mut uv_scale, mut uv_offset) = if let Some([u0, v0, u1, v1]) = uv_rect {
        let du = (u1 - u0).abs().max(1e-6);
        let dv = (v1 - v0).abs().max(1e-6);
        ([du, dv], [u0.min(u1), v0.min(v1)])
    } else if let Some((cx, cy)) = cell {
        let (gc, gr) = grid.unwrap_or_else(|| assets::sprite_sheet_dims(texture));
        let cols = gc.max(1);
        let rows = gr.max(1);
        let (col, row) = if cy == u32::MAX {
            let idx = cx;
            (idx % cols, (idx / cols).min(rows.saturating_sub(1)))
        } else {
            (
                cx.min(cols.saturating_sub(1)),
                cy.min(rows.saturating_sub(1)),
            )
        };
        let s = [1.0 / cols as f32, 1.0 / rows as f32];
        let o = [col as f32 * s[0], row as f32 * s[1]];
        (s, o)
    } else {
        ([1.0, 1.0], [0.0, 0.0])
    };

    uv_offset[0] += uv_scale[0] * cl;
    uv_offset[1] += uv_scale[1] * ct;
    uv_scale[0] *= (1.0 - cl - cr).max(0.0);
    uv_scale[1] *= (1.0 - ct - cb).max(0.0);

    if flip_x {
        uv_offset[0] += uv_scale[0];
        uv_scale[0] = -uv_scale[0];
    }
    if flip_y {
        uv_offset[1] += uv_scale[1];
        uv_scale[1] = -uv_scale[1];
    }

    if let Some(vel) = texcoordvelocity {
        uv_offset[0] += vel[0] * total_elapsed;
        uv_offset[1] += vel[1] * total_elapsed;
    }

    (uv_scale, uv_offset)
}

#[inline(always)]
fn push_sprite<'a>(
    out: &mut Vec<renderer::RenderObject<'a>>,
    camera: u8,
    rect: SmRect,
    m: &Metrics,
    is_solid: bool,
    texture_id: &'a str,
    tint: [f32; 4],
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    flip_x: bool,
    flip_y: bool,
    cropleft: f32,
    cropright: f32,
    croptop: f32,
    cropbottom: f32,
    fadeleft: f32,
    faderight: f32,
    fadetop: f32,
    fadebottom: f32,
    blend: BlendMode,
    rot_x_deg: f32,
    rot_y_deg: f32,
    rot_z_deg: f32,
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
    texcoordvelocity: Option<[f32; 2]>,
    total_elapsed: f32,
) {
    if tint[3] <= 0.0 {
        return;
    }

    let (cl, cr, ct, cb) = clamp_crop_fractions(cropleft, cropright, croptop, cropbottom);

    let (base_center, base_size) = sm_rect_to_world_center_size(rect, m);
    if base_size.x <= 0.0 || base_size.y <= 0.0 {
        return;
    }

    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= 0.0 || sy_crop <= 0.0 {
        return;
    }

    // StepMania parity: crop shifts geometry toward the un-cropped side(s).
    // (This matches Sprite::DrawTexture(), which moves quad vertices instead of the actor.)
    let center_x = ((cl - cr) * base_size.x).mul_add(0.5, base_center.x);
    let center_y = ((cb - ct) * base_size.y).mul_add(0.5, base_center.y);
    let size_x = base_size.x * sx_crop;
    let size_y = base_size.y * sy_crop;

    let (uv_scale, uv_offset) = if is_solid {
        ([1.0, 1.0], [0.0, 0.0])
    } else {
        calculate_uvs(
            texture_id,
            uv_rect,
            cell,
            grid,
            flip_x,
            flip_y,
            cl,
            cr,
            ct,
            cb,
            texcoordvelocity,
            total_elapsed,
        )
    };

    let fl = fadeleft.clamp(0.0, 1.0);
    let fr = faderight.clamp(0.0, 1.0);
    let ft = fadetop.clamp(0.0, 1.0);
    let fb = fadebottom.clamp(0.0, 1.0);

    // StepMania parity (Sprite::DrawPrimitives edge-fade behavior):
    // - Fade distances are specified in the *pre-crop* [0..1] space.
    // - Visible (post-crop) fraction is `(1 - crop_a - crop_b)`.
    // - Negative crop values can "cancel" fade (used by Simply Love transitions).
    let mut fl_size = (fl + cropleft.min(0.0)).max(0.0);
    let mut fr_size = (fr + cropright.min(0.0)).max(0.0);
    let mut ft_size = (ft + croptop.min(0.0)).max(0.0);
    let mut fb_size = (fb + cropbottom.min(0.0)).max(0.0);

    let sum_x = fl_size + fr_size;
    if sum_x > 0.0 && sx_crop < sum_x {
        let s = sx_crop / sum_x;
        fl_size *= s;
        fr_size *= s;
    }

    let sum_y = ft_size + fb_size;
    if sum_y > 0.0 && sy_crop < sum_y {
        let s = sy_crop / sum_y;
        ft_size *= s;
        fb_size *= s;
    }

    let mut fl_eff = (fl_size / sx_crop).clamp(0.0, 1.0);
    let mut fr_eff = (fr_size / sx_crop).clamp(0.0, 1.0);
    let mut ft_eff = (ft_size / sy_crop).clamp(0.0, 1.0);
    let mut fb_eff = (fb_size / sy_crop).clamp(0.0, 1.0);

    if flip_x {
        std::mem::swap(&mut fl_eff, &mut fr_eff);
    }
    if flip_y {
        std::mem::swap(&mut ft_eff, &mut fb_eff);
    }

    // Matrix = T * R * S
    // SM->world flips Y, so rotationx sign flips; rotationy/z keep sign.
    let transform = {
        let rx = Matrix4::from_angle_x(Rad((-rot_x_deg).to_radians()));
        let ry = Matrix4::from_angle_y(Rad(rot_y_deg.to_radians()));
        let rz = Matrix4::from_angle_z(Rad(rot_z_deg.to_radians()));
        let r = rx * ry * rz;
        let s = Matrix4::from_nonuniform_scale(size_x, size_y, 1.0);
        let t = Matrix4::from_translation(Vector3::new(center_x, center_y, 0.0));
        t * r * s
    };

    let final_texture_id = if is_solid {
        std::borrow::Cow::Borrowed("__white")
    } else {
        std::borrow::Cow::Borrowed(texture_id)
    };

    out.push(renderer::RenderObject {
        object_type: renderer::ObjectType::Sprite {
            texture_id: final_texture_id,
            tint,
            uv_scale,
            uv_offset,
            local_offset,
            local_offset_rot_sin_cos,
            edge_fade: [fl_eff, fr_eff, ft_eff, fb_eff],
        },
        texture_handle: renderer::INVALID_TEXTURE_HANDLE,
        transform,
        blend,
        z: 0,
        order: 0,
        camera,
    });
}

#[inline(always)]
#[must_use]
const fn clamp_crop_fractions(l: f32, r: f32, t: f32, b: f32) -> (f32, f32, f32, f32) {
    (
        l.clamp(0.0, 1.0),
        r.clamp(0.0, 1.0),
        t.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
    )
}

#[inline(always)]
#[must_use]
fn lrint_ties_even(v: f32) -> f32 {
    if !v.is_finite() {
        return 0.0;
    }
    // Fast path: already an integer (including -0.0)
    if v.fract() == 0.0 {
        return v;
    }

    let floor = v.floor();
    let frac = v - floor;

    if frac < 0.5 {
        floor
    } else if frac > 0.5 {
        floor + 1.0
    } else {
        // frac == 0.5 exactly: ties-to-even
        // Use i64 for parity check to avoid edge overflow on extreme values.
        let f_even = ((floor as i64) & 1) == 0;
        if f_even { floor } else { floor + 1.0 }
    }
}

#[inline(always)]
#[must_use]
const fn quantize_up_even_i32(v: i32) -> i32 {
    if v <= 0 {
        0
    } else if (v & 1) != 0 {
        v + 1
    } else {
        v
    }
}

fn layout_text<'a>(
    out: &mut Vec<RenderObject<'a>>,
    font: &'a font::Font,
    fonts: &'a HashMap<&'static str, font::Font>,
    content: &actors::TextContent,
    _px_size: f32,
    scale: [f32; 2],
    fit_width: Option<f32>,
    fit_height: Option<f32>,
    wrap_width_pixels: Option<i32>,
    max_width: Option<f32>,
    max_height: Option<f32>,
    // NEW: StepMania order semantics (per axis)
    max_w_pre_zoom: bool,
    max_h_pre_zoom: bool,
    parent: SmRect,
    align: [f32; 2],
    offset: [f32; 2],
    text_align: actors::TextAlign,
    m: &Metrics,
    text_cache: &mut TextLayoutCache,
) {
    if content.as_str().is_empty() {
        return;
    }
    let layout = text_cache.get_or_build(font, fonts, content, wrap_width_pixels);
    let num_lines = layout.lines.len();
    if num_lines == 0 {
        return;
    }
    let max_logical_width_i = layout.max_logical_width_i;
    let block_w_logical_even = quantize_up_even_i32(max_logical_width_i) as f32;

    // 2) Unscaled block cap height + line spacing in logical units
    let cap_height = if font.height > 0 {
        font.height as f32
    } else {
        font.line_spacing as f32
    };

    let block_h_logical_i = if num_lines > 1 {
        font.height + ((num_lines - 1) as i32 * font.line_spacing)
    } else {
        font.height
    };
    let block_h_logical = if block_h_logical_i > 0 {
        block_h_logical_i as f32
    } else {
        cap_height
    };

    // 3) Fit scaling (zoomto...) preserves aspect ratio
    let s_w_fit = fit_width.map_or(f32::INFINITY, |w| {
        if block_w_logical_even > 0.0 {
            w / block_w_logical_even
        } else {
            1.0
        }
    });
    let s_h_fit = fit_height.map_or(f32::INFINITY, |h| {
        if block_h_logical > 0.0 {
            h / block_h_logical
        } else {
            1.0
        }
    });
    let fit_s = if s_w_fit.is_infinite() && s_h_fit.is_infinite() {
        1.0
    } else {
        s_w_fit.min(s_h_fit).max(0.0)
    };

    // 4) Reference sizes before/after zoom (but before max clamp)
    let width_before_zoom = block_w_logical_even * fit_s;
    let height_before_zoom = block_h_logical * fit_s;

    let width_after_zoom = width_before_zoom * scale[0];
    let height_after_zoom = height_before_zoom * scale[1];

    // 5) Decide the clamp denominators per axis based on order flags
    let denom_w_for_max = if max_w_pre_zoom {
        width_before_zoom
    } else {
        width_after_zoom
    };
    let denom_h_for_max = if max_h_pre_zoom {
        height_before_zoom
    } else {
        height_after_zoom
    };

    // 6) Compute per-axis extra downscale from max constraints
    let max_s_w = max_width.map_or(1.0, |mw| {
        if denom_w_for_max > mw {
            (mw / denom_w_for_max).max(0.0)
        } else {
            1.0
        }
    });
    let max_s_h = max_height.map_or(1.0, |mh| {
        if denom_h_for_max > mh {
            (mh / denom_h_for_max).max(0.0)
        } else {
            1.0
        }
    });

    // 7) Final per-axis scales: fit * zoom * (potential extra downscale)
    let sx = scale[0] * fit_s * max_s_w;
    let sy = scale[1] * fit_s * max_s_h;
    if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
        return;
    }

    // 8) Pixel rounding/snapping
    let block_w_px = block_w_logical_even * sx;
    let block_h_px = block_h_logical * sy;

    // 9) Place the block, compute baseline (unchanged)
    let block_left_sm = align[0].mul_add(-block_w_px, parent.x + offset[0]);
    let block_top_sm = align[1].mul_add(-block_h_px, parent.y + offset[1]);
    let block_center_x = 0.5f32.mul_add(block_w_px, block_left_sm);
    let block_center_y = 0.5f32.mul_add(block_h_px, block_top_sm);

    let mut pen_y_logical = lrint_ties_even(-(block_h_logical_i as f32) * 0.5) as i32;
    let line_padding = font.line_spacing - font.height;

    #[inline(always)]
    fn start_x_logical(align: actors::TextAlign, block_w_logical: f32, line_w_logical: f32) -> i32 {
        let align_value = match align {
            actors::TextAlign::Left => 0.0,
            actors::TextAlign::Center => 0.5,
            actors::TextAlign::Right => 1.0,
        };
        let start = (-0.5f32).mul_add(
            block_w_logical,
            align_value * (block_w_logical - line_w_logical),
        );
        lrint_ties_even(start) as i32
    }

    #[inline(always)]
    fn logical_to_world(center: f32, logical: f32, scale: f32) -> f32 {
        logical.mul_add(scale, center)
    }

    out.reserve(layout.glyph_count);

    for line in &layout.lines {
        pen_y_logical += font.height;
        let baseline_local_logical = pen_y_logical as f32;
        let mut pen_x_logical =
            start_x_logical(text_align, block_w_logical_even, line.width_i32 as f32);

        for glyph in &line.glyphs {
            let quad_w = glyph.size[0] * sx;
            let quad_h = glyph.size[1] * sy;

            if glyph.draw_quad && quad_w.abs() >= 1e-6 && quad_h.abs() >= 1e-6 {
                let quad_x_logical = pen_x_logical as f32 + glyph.offset[0];
                let quad_y_logical = baseline_local_logical + glyph.offset[1];

                let quad_x_sm = logical_to_world(block_center_x, quad_x_logical, sx);
                let quad_y_sm = logical_to_world(block_center_y, quad_y_logical, sy);

                let center_x = m.left + quad_x_sm + quad_w * 0.5;
                let center_y = m.top - (quad_y_sm + quad_h * 0.5);

                // Optimization: T * S manually
                // c0 = [w, 0, 0, 0]
                // c1 = [0, h, 0, 0]
                // c2 = [0, 0, 1, 0]
                // c3 = [tx, ty, 0, 1]
                let transform = Matrix4::new(
                    quad_w, 0.0, 0.0, 0.0, 0.0, quad_h, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, center_x,
                    center_y, 0.0, 1.0,
                );

                out.push(RenderObject {
                    object_type: renderer::ObjectType::Sprite {
                        texture_id: std::borrow::Cow::Borrowed(unsafe {
                            str_from_cached_ptr(glyph.texture_key)
                        }),
                        tint: [1.0; 4],
                        uv_scale: glyph.uv_scale,
                        uv_offset: glyph.uv_offset,
                        local_offset: [0.0, 0.0],
                        local_offset_rot_sin_cos: [0.0, 1.0],
                        edge_fade: [0.0; 4],
                    },
                    texture_handle: renderer::INVALID_TEXTURE_HANDLE,
                    transform,
                    blend: BlendMode::Alpha,
                    z: 0,
                    order: 0,
                    camera: 0,
                });
            }

            pen_x_logical += glyph.advance_i32;
        }
        pen_y_logical += line_padding;
    }
}

#[inline(always)]
fn sm_rect_to_world_center_size(rect: SmRect, m: &Metrics) -> (Vector2<f32>, Vector2<f32>) {
    (
        Vector2::new(
            0.5f32.mul_add(rect.w, m.left + rect.x),
            m.top - 0.5f32.mul_add(rect.h, rect.y),
        ),
        Vector2::new(rect.w, rect.h),
    )
}

#[derive(Clone, Copy, Debug)]
struct WorldRect {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
}

#[inline(always)]
fn sm_rect_to_world_edges(rect: SmRect, m: &Metrics) -> WorldRect {
    let left = m.left + rect.x;
    let right = rect.w.mul_add(1.0, left);

    let top = m.top - rect.y;
    let bottom = top - rect.h;

    WorldRect {
        left,
        right,
        bottom,
        top,
    }
}

fn clip_objects_range_to_world_masks(
    objects: &mut Vec<RenderObject<'_>>,
    start: usize,
    masks: &[WorldRect],
) {
    if start >= objects.len() {
        return;
    }
    if masks.is_empty() {
        objects.truncate(start);
        return;
    }
    if let [mask] = masks {
        clip_objects_range_to_world_rect(objects, start, *mask);
        return;
    }
    let len = objects.len();
    let mut write = start;
    for read in start..len {
        let keep = {
            let obj = &mut objects[read];
            clip_object_to_world_masks(obj, masks)
        };
        if keep {
            if write != read {
                objects.swap(write, read);
            }
            write += 1;
        }
    }
    objects.truncate(write);
}

#[inline(always)]
fn sprite_object_world_area(obj: &RenderObject<'_>) -> f32 {
    match &obj.object_type {
        renderer::ObjectType::Sprite { .. } => {
            let t = &obj.transform;
            (t.x.x * t.y.y).abs()
        }
        renderer::ObjectType::TexturedMesh { vertices, .. } => {
            if vertices.len() < 3 {
                return 0.0;
            }
            let t = &obj.transform;
            let mut area = 0.0_f32;
            let mut i = 0usize;
            while i + 2 < vertices.len() {
                let p0 = world_xy(t, vertices[i].pos);
                let p1 = world_xy(t, vertices[i + 1].pos);
                let p2 = world_xy(t, vertices[i + 2].pos);
                let a = (p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0]);
                area += 0.5 * a.abs();
                i += 3;
            }
            area
        }
        renderer::ObjectType::Mesh { .. } => 0.0,
    }
}

fn clip_object_to_world_masks(obj: &mut RenderObject<'_>, masks: &[WorldRect]) -> bool {
    let mut best_obj: Option<RenderObject<'_>> = None;
    let mut best_area = -1.0_f32;
    for &mask in masks {
        let mut candidate = obj.clone();
        if !clip_sprite_object_to_world_rect(&mut candidate, mask) {
            continue;
        }
        let area = sprite_object_world_area(&candidate);
        if area > best_area {
            best_area = area;
            best_obj = Some(candidate);
        }
    }
    if let Some(chosen) = best_obj {
        *obj = chosen;
        true
    } else {
        false
    }
}

fn clip_objects_range_to_world_rect(
    objects: &mut Vec<RenderObject<'_>>,
    start: usize,
    clip: WorldRect,
) {
    if start >= objects.len() {
        return;
    }
    if clip.left >= clip.right || clip.bottom >= clip.top {
        objects.truncate(start);
        return;
    }

    let len = objects.len();
    let mut write = start;
    for read in start..len {
        let keep = {
            let obj = &mut objects[read];
            clip_sprite_object_to_world_rect(obj, clip)
        };
        if keep {
            if write != read {
                objects.swap(write, read);
            }
            write += 1;
        }
    }
    objects.truncate(write);
}

fn clip_sprite_object_to_world_rect(obj: &mut RenderObject<'_>, clip: WorldRect) -> bool {
    if clip.left >= clip.right || clip.bottom >= clip.top {
        return false;
    }
    let renderer::ObjectType::Sprite {
        uv_scale,
        uv_offset,
        ..
    } = &mut obj.object_type
    else {
        // Only sprite objects support clip-by-adjusting-UV today.
        return true;
    };

    let eps = 1e-6;
    let t = &obj.transform;
    if t.x.y.abs() > eps || t.y.x.abs() > eps || t.x.z.abs() > eps || t.y.z.abs() > eps {
        return clip_rotated_sprite_object_to_world_rect(obj, clip);
    }

    let w = t.x.x;
    let h = t.y.y;
    if w <= eps || h <= eps {
        return false;
    }

    let cx = t.w.x;
    let cy = t.w.y;

    let half_w = w * 0.5;
    let half_h = h * 0.5;

    let left = cx - half_w;
    let right = cx + half_w;
    let bottom = cy - half_h;
    let top = cy + half_h;

    let inter_left = left.max(clip.left);
    let inter_right = right.min(clip.right);
    let inter_bottom = bottom.max(clip.bottom);
    let inter_top = top.min(clip.top);
    if inter_left >= inter_right || inter_bottom >= inter_top {
        return false;
    }

    let inv_w = 1.0 / w;
    let inv_h = 1.0 / h;

    let cl = ((inter_left - left) * inv_w).clamp(0.0, 1.0);
    let cr = ((right - inter_right) * inv_w).clamp(0.0, 1.0);
    let cb = ((inter_bottom - bottom) * inv_h).clamp(0.0, 1.0);
    let ct = ((top - inter_top) * inv_h).clamp(0.0, 1.0);

    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= eps || sy_crop <= eps {
        return false;
    }

    uv_offset[0] += uv_scale[0] * cl;
    uv_offset[1] += uv_scale[1] * ct;
    uv_scale[0] *= sx_crop;
    uv_scale[1] *= sy_crop;

    let center_x = ((cl - cr) * w).mul_add(0.5, cx);
    let center_y = ((cb - ct) * h).mul_add(0.5, cy);
    let new_w = w * sx_crop;
    let new_h = h * sy_crop;

    obj.transform = Matrix4::new(
        new_w, 0.0, 0.0, 0.0, //
        0.0, new_h, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        center_x, center_y, 0.0, 1.0,
    );

    true
}

#[derive(Clone, Copy)]
struct ClipVertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

#[inline(always)]
fn world_xy(t: &Matrix4<f32>, p: [f32; 2]) -> [f32; 2] {
    [
        t.x.x.mul_add(p[0], t.y.x.mul_add(p[1], t.w.x)),
        t.x.y.mul_add(p[0], t.y.y.mul_add(p[1], t.w.y)),
    ]
}

#[inline(always)]
fn lerp_clip(a: ClipVertex, b: ClipVertex, t: f32) -> ClipVertex {
    let t = t.clamp(0.0, 1.0);
    ClipVertex {
        pos: [
            (b.pos[0] - a.pos[0]).mul_add(t, a.pos[0]),
            (b.pos[1] - a.pos[1]).mul_add(t, a.pos[1]),
        ],
        uv: [
            (b.uv[0] - a.uv[0]).mul_add(t, a.uv[0]),
            (b.uv[1] - a.uv[1]).mul_add(t, a.uv[1]),
        ],
    }
}

fn clip_poly_edge(
    poly: &[ClipVertex],
    axis: usize,
    bound: f32,
    keep_greater: bool,
) -> Vec<ClipVertex> {
    if poly.is_empty() {
        return vec![];
    }
    let mut out = Vec::with_capacity(poly.len() + 2);
    let mut prev = poly[poly.len() - 1];
    let mut prev_in = if keep_greater {
        prev.pos[axis] >= bound
    } else {
        prev.pos[axis] <= bound
    };

    for &curr in poly {
        let curr_in = if keep_greater {
            curr.pos[axis] >= bound
        } else {
            curr.pos[axis] <= bound
        };
        if prev_in && curr_in {
            out.push(curr);
        } else if prev_in && !curr_in {
            let denom = curr.pos[axis] - prev.pos[axis];
            if denom.abs() > 1e-6 {
                let t = (bound - prev.pos[axis]) / denom;
                out.push(lerp_clip(prev, curr, t));
            }
        } else if !prev_in && curr_in {
            let denom = curr.pos[axis] - prev.pos[axis];
            if denom.abs() > 1e-6 {
                let t = (bound - prev.pos[axis]) / denom;
                out.push(lerp_clip(prev, curr, t));
            }
            out.push(curr);
        }
        prev = curr;
        prev_in = curr_in;
    }
    out
}

fn clip_polygon_to_world_rect(poly: &[ClipVertex], clip: WorldRect) -> Vec<ClipVertex> {
    let mut p = clip_poly_edge(poly, 0, clip.left, true);
    p = clip_poly_edge(&p, 0, clip.right, false);
    p = clip_poly_edge(&p, 1, clip.bottom, true);
    clip_poly_edge(&p, 1, clip.top, false)
}

fn clip_rotated_sprite_object_to_world_rect(obj: &mut RenderObject<'_>, clip: WorldRect) -> bool {
    let (texture_id, tint, uv_scale, uv_offset) = match &obj.object_type {
        renderer::ObjectType::Sprite {
            texture_id,
            tint,
            uv_scale,
            uv_offset,
            ..
        } => (texture_id.to_string(), *tint, *uv_scale, *uv_offset),
        _ => return true,
    };

    let t = obj.transform;
    let quad = [
        ([-0.5_f32, -0.5_f32], [0.0_f32, 1.0_f32]),
        ([0.5_f32, -0.5_f32], [1.0_f32, 1.0_f32]),
        ([0.5_f32, 0.5_f32], [1.0_f32, 0.0_f32]),
        ([-0.5_f32, 0.5_f32], [0.0_f32, 0.0_f32]),
    ];
    let mut poly = Vec::with_capacity(4);
    for (local, base_uv) in quad {
        poly.push(ClipVertex {
            pos: world_xy(&t, local),
            uv: [
                uv_offset[0] + base_uv[0] * uv_scale[0],
                uv_offset[1] + base_uv[1] * uv_scale[1],
            ],
        });
    }

    let clipped = clip_polygon_to_world_rect(&poly, clip);
    if clipped.len() < 3 {
        return false;
    }

    let mut out = Vec::with_capacity((clipped.len() - 2) * 3);
    let base = clipped[0];
    let mut i = 1usize;
    while i + 1 < clipped.len() {
        for v in [base, clipped[i], clipped[i + 1]] {
            out.push(renderer::TexturedMeshVertex {
                pos: v.pos,
                uv: v.uv,
                tex_matrix_scale: [1.0, 1.0],
                color: tint,
            });
        }
        i += 1;
    }

    obj.object_type = renderer::ObjectType::TexturedMesh {
        texture_id: std::borrow::Cow::Owned(texture_id),
        vertices: std::borrow::Cow::Owned(out),
        mode: renderer::MeshMode::Triangles,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
    };
    obj.transform = Matrix4::from_scale(1.0);
    true
}

#[cfg(test)]
mod tests {
    use super::wrap_text_lines_by_words;

    #[test]
    fn wrapwidthpixels_wraps_on_spaces() {
        let lines = wrap_text_lines_by_words("A BB CCC", 3, 1, |word| word.len() as i32);
        assert_eq!(lines, vec!["A", "BB", "CCC"]);
    }

    #[test]
    fn wrapwidthpixels_keeps_empty_lines() {
        let lines = wrap_text_lines_by_words("AA\n\nBB CC", 5, 1, |word| word.len() as i32);
        assert_eq!(lines, vec!["AA", "", "BB", "CC"]);
    }

    #[test]
    fn wrapwidthpixels_keeps_long_word_on_own_line() {
        let lines = wrap_text_lines_by_words("AAAA BB", 3, 1, |word| word.len() as i32);
        assert_eq!(lines, vec!["AAAA", "BB"]);
    }
}
