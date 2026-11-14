//! Raw Lua fields that per-frame code asks every actor for.
//!
//! The actor's table stays the only place these fields live. What a read
//! returned is remembered beside the actor's native state and reused for as
//! long as the VM reports that the table's keyed entries are unchanged, so a
//! frame in which nothing wrote to an actor does not read its table again.
use super::*;
use smallvec::SmallVec;

/// One of the fields below.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Field(u8);

macro_rules! fields {
    ($($kind:ident $ident:ident = $name:literal,)*) => {
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        enum Index { $($ident,)* COUNT }
        impl Field {
            $(pub(super) const $ident: Field = Field(Index::$ident as u8);)*
        }
        static NAMES: [&str; Index::COUNT as usize] = [$($name,)*];
        // Properties the live pose owns during playback.
        static NATIVE: [bool; Index::COUNT as usize] = [$(fields!(@native $kind),)*];
    };
    (@native native) => { true };
    (@native plain) => { false };
}

fields! {
    plain ACTOR_TYPE = "__songlua_actor_type",
    plain STATE_CULL_MODE = "__songlua_state_cull_mode",
    plain HIBERNATE_SECONDS = "__songlua_hibernate_seconds",
    plain THEME_HIBERNATING = "__songlua_theme_hibernating",
    native STATE_AUX = "__songlua_state_aux",
    plain VISIBLE = "__songlua_visible",
    plain STATE_SPRITE_TEXTURE = "__songlua_state_sprite_texture",
    plain AFT_ALLOCATION = "__songlua_aft_allocation",
    native STATE_X = "__songlua_state_x",
    native STATE_Y = "__songlua_state_y",
    native STATE_Z = "__songlua_state_z",
    plain STATE_Z_BIAS = "__songlua_state_z_bias",
    native STATE_HALIGN = "__songlua_state_halign",
    native STATE_VALIGN = "__songlua_state_valign",
    native STATE_CROPLEFT = "__songlua_state_cropleft",
    native STATE_CROPRIGHT = "__songlua_state_cropright",
    native STATE_CROPTOP = "__songlua_state_croptop",
    native STATE_CROPBOTTOM = "__songlua_state_cropbottom",
    native STATE_FADELEFT = "__songlua_state_fadeleft",
    native STATE_FADERIGHT = "__songlua_state_faderight",
    native STATE_FADETOP = "__songlua_state_fadetop",
    native STATE_FADEBOTTOM = "__songlua_state_fadebottom",
    native STATE_ZOOM = "__songlua_state_zoom",
    native STATE_ZOOM_X = "__songlua_state_zoom_x",
    native STATE_ZOOM_Y = "__songlua_state_zoom_y",
    native STATE_ZOOM_Z = "__songlua_state_zoom_z",
    native STATE_BASEZOOM = "__songlua_state_basezoom",
    native STATE_BASEZOOM_X = "__songlua_state_basezoom_x",
    native STATE_BASEZOOM_Y = "__songlua_state_basezoom_y",
    native STATE_BASEZOOM_Z = "__songlua_state_basezoom_z",
    plain STATE_BASE_ALPHA = "__songlua_state_base_alpha",
    native STATE_ROT_X_DEG = "__songlua_state_rot_x_deg",
    native STATE_ROT_Y_DEG = "__songlua_state_rot_y_deg",
    native STATE_ROT_Z_DEG = "__songlua_state_rot_z_deg",
    native STATE_SKEW_X = "__songlua_state_skew_x",
    native STATE_SKEW_Y = "__songlua_state_skew_y",
    plain STATE_EFFECT_PERIOD = "__songlua_state_effect_period",
    plain STATE_EFFECT_OFFSET = "__songlua_state_effect_offset",
    plain STATE_TEXT_DISTORTION = "__songlua_state_text_distortion",
    plain STATE_SPRITE_PLAYBACK_RATE = "__songlua_state_sprite_playback_rate",
    plain STATE_SPRITE_STATE_DELAY = "__songlua_state_sprite_state_delay",
    plain STATE_MAX_WIDTH = "__songlua_state_max_width",
    plain STATE_MAX_HEIGHT = "__songlua_state_max_height",
    native STATE_DRAW_BY_Z_POSITION = "__songlua_state_draw_by_z_position",
    plain STATE_AFT_PRESERVE = "__songlua_state_aft_preserve",
    plain STATE_AFT_CREATED = "__songlua_state_aft_created",
    plain STATE_UPPERCASE = "__songlua_state_uppercase",
    native STATE_MASK_SOURCE = "__songlua_state_mask_source",
    native STATE_MASK_DEST = "__songlua_state_mask_dest",
    native STATE_DEPTH_TEST = "__songlua_state_depth_test",
    native STATE_DEPTH_WRITE = "__songlua_state_depth_write",
    native STATE_DEPTH_TEST_ON_FAIL = "__songlua_state_depth_test_on_fail",
    plain STATE_VIBRATE = "__songlua_state_vibrate",
    plain STATE_RAINBOW = "__songlua_state_rainbow",
    plain STATE_RAINBOW_SCROLL = "__songlua_state_rainbow_scroll",
    plain STATE_TEXT_JITTER = "__songlua_state_text_jitter",
    plain STATE_MULT_ATTRS_WITH_DIFFUSE = "__songlua_state_mult_attrs_with_diffuse",
    plain STATE_SPRITE_ANIMATE = "__songlua_state_sprite_animate",
    plain STATE_SPRITE_LOOP = "__songlua_state_sprite_loop",
    plain STATE_MAX_W_PRE_ZOOM = "__songlua_state_max_w_pre_zoom",
    plain STATE_MAX_H_PRE_ZOOM = "__songlua_state_max_h_pre_zoom",
    plain STATE_MAX_DIMENSION_USES_ZOOM = "__songlua_state_max_dimension_uses_zoom",
    plain STATE_DECODE_MOVIE = "__songlua_state_decode_movie",
    native STATE_TEXTURE_FILTERING = "__songlua_state_texture_filtering",
    native STATE_TEXTURE_WRAPPING = "__songlua_state_texture_wrapping",
    plain STATE_EFFECT_TIMER = "__songlua_state_effect_timer",
    plain STATE_EFFECT_NO_OFFSET = "__songlua_state_effect_no_offset",
    plain STATE_FOV = "__songlua_state_fov",
    plain FOV = "FOV",
    plain STATE_DRAW_ORDER = "__songlua_state_draw_order",
    plain STATE_VERT_SPACING = "__songlua_state_vert_spacing",
    plain STATE_WRAP_WIDTH_PIXELS = "__songlua_state_wrap_width_pixels",
    plain STATE_SPRITE_STATE_INDEX = "__songlua_state_sprite_state_index",
    native STATE_DIFFUSE = "__songlua_state_diffuse",
    plain STATE_SHADOW_LEN = "__songlua_state_shadow_len",
    plain STATE_SHADOW_COLOR = "__songlua_state_shadow_color",
    native STATE_GLOW = "__songlua_state_glow",
    plain STATE_VANISHPOINT = "__songlua_state_vanishpoint",
    native STATE_QUATERNION = "__songlua_state_quaternion",
    plain STATE_BASE_ROTATION = "__songlua_state_base_rotation",
    plain STATE_EFFECT_MAGNITUDE = "__songlua_state_effect_magnitude",
    plain STATE_EFFECT_COLOR1 = "__songlua_state_effect_color1",
    plain STATE_EFFECT_COLOR2 = "__songlua_state_effect_color2",
    plain STATE_EFFECT_TIMING = "__songlua_state_effect_timing",
    plain STATE_TEXCOORD_OFFSET = "__songlua_state_texcoord_offset",
    plain STATE_CUSTOM_TEXTURE_RECT = "__songlua_state_custom_texture_rect",
    plain STATE_TEXCOORD_VELOCITY = "__songlua_state_texcoord_velocity",
    plain STATE_SIZE = "__songlua_state_size",
    plain STATE_STRETCH_RECT = "__songlua_state_stretch_rect",
    native STATE_VERTEX_COLORS = "__songlua_state_vertex_colors",
    plain STATE_TEXT_ALIGN = "__songlua_state_text_align",
    plain STATE_EFFECT_CLOCK = "__songlua_state_effect_clock",
    plain STATE_TEXT_GLOW_MODE = "__songlua_state_text_glow_mode",
    plain STATE_BLEND = "__songlua_state_blend",
    plain STATE_EFFECT_MODE = "__songlua_state_effect_mode",
    plain LIVE_INACTIVE = "__songlua_live_inactive",
    plain LIVE_DELTA = "__songlua_live_delta",
    plain UPDATE_RATE = "__songlua_update_rate",
    plain UPDATE_PENDING = "__songlua_update_pending",
    plain UPDATE_PREVIOUS = "__songlua_update_previous",
    plain UPDATE_FUNCTION = "__songlua_update_function",
}

const WORDS: usize = 2;
const _: () = assert!(Index::COUNT as usize <= WORDS * 64);

impl Field {
    pub(super) fn name(self) -> &'static str {
        NAMES[self.0 as usize]
    }
    /// Whether the live pose owns this property, so that its raw field is not
    /// read during playback.
    pub(super) fn native(self) -> bool {
        NATIVE[self.0 as usize]
    }
    fn bit(self) -> (usize, u64) {
        (self.0 as usize / 64, 1 << (self.0 % 64))
    }
}

/// What one table held for these fields when its keyed entries last changed.
#[derive(Default)]
pub(super) struct Known {
    // The table and the version of its keyed entries the answers belong to.
    // Nothing is known about any other table or version.
    table: Option<TableId>,
    version: u32,
    read: [u64; WORDS],
    present: [u64; WORDS],
    values: SmallVec<[(Field, Value); 4]>,
}
impl Known {
    fn at(&mut self, table: TableId, version: u32) -> &mut Self {
        if self.table != Some(table) || self.version != version {
            self.table = Some(table);
            self.version = version;
            self.read = [0; WORDS];
            self.present = [0; WORDS];
            self.values.clear();
        }
        self
    }
    fn find(&self, field: Field) -> Option<Value> {
        let (word, bit) = field.bit();
        if self.read[word] & bit == 0 {
            return None;
        }
        if self.present[word] & bit == 0 {
            return Some(Value::Nil);
        }
        self.values
            .iter()
            .find(|(known, _)| *known == field)
            .map(|(_, value)| *value)
    }
    fn record(&mut self, field: Field, value: Value) {
        let (word, bit) = field.bit();
        self.read[word] |= bit;
        if value != Value::Nil {
            self.present[word] |= bit;
            self.values.push((field, value));
        }
    }
}

// The table is the definition: debug builds check every remembered answer
// against it.
#[inline]
fn check(call: &mut HostCall<'_>, table: TableId, field: Field, value: Value) -> Result<(), Error> {
    if cfg!(debug_assertions) {
        let held = call.raw_table_get_static(table, field.name().as_bytes())?;
        let same = match (value, held) {
            (Value::Number(a), Value::Number(b)) => a.to_bits() == b.to_bits(),
            (a, b) => a == b,
        };
        assert!(
            same,
            "stale {}: {value:?}, the table holds {held:?}",
            field.name()
        );
    }
    Ok(())
}

/// `table[field]` without metamethods. A table that is an owned actor answers
/// from what it is known to hold; any other table is read.
pub(super) fn get(call: &mut HostCall<'_>, table: TableId, field: Field) -> Result<Value, Error> {
    let version = call.raw_fields_version(table)?;
    let Some(actor) = call.store().actors.get_mut(&table) else {
        return call.raw_table_get_static(table, field.name().as_bytes());
    };
    if let Some(value) = actor.known.at(table, version).find(field) {
        check(call, table, field, value)?;
        return Ok(value);
    }
    let value = call.raw_table_get_static(table, field.name().as_bytes())?;
    // The read ran no Lua, so the version still describes the table.
    if let Some(actor) = call.store().actors.get_mut(&table) {
        actor.known.at(table, version).record(field, value);
    }
    Ok(value)
}

/// `get` for several fields of one table. The fields are read together, so
/// nothing may write to the table between the reads this replaces.
pub(super) fn get_all<const N: usize>(
    call: &mut HostCall<'_>,
    table: TableId,
    fields: [Field; N],
) -> Result<[Value; N], Error> {
    let version = call.raw_fields_version(table)?;
    let mut values = [Value::Nil; N];
    if let Some(actor) = call.store().actors.get_mut(&table) {
        let known = actor.known.at(table, version);
        let mut remembered = true;
        for (value, field) in values.iter_mut().zip(fields) {
            match known.find(field) {
                Some(found) => *value = found,
                None => remembered = false,
            }
        }
        if remembered {
            for (value, field) in values.iter().zip(fields) {
                check(call, table, field, *value)?;
            }
            return Ok(values);
        }
    }
    for (value, field) in values.iter_mut().zip(fields) {
        *value = get(call, table, field)?;
    }
    Ok(values)
}

/// Many reads of one table's fields. What the actor knows of its table is
/// held here meanwhile, so each read is `get` without finding the actor
/// again; `finish` hands it back.
pub(super) struct Reader {
    table: TableId,
    // `None` for a table that is not an owned actor.
    known: Option<Known>,
}
impl Reader {
    pub(super) fn new(call: &mut HostCall<'_>, table: TableId) -> Self {
        let known = call
            .store()
            .actors
            .get_mut(&table)
            .map(|actor| std::mem::take(&mut actor.known));
        Self { table, known }
    }
    #[inline]
    pub(super) fn get(&mut self, call: &mut HostCall<'_>, field: Field) -> Result<Value, Error> {
        let Some(known) = &mut self.known else {
            return call.raw_table_get_static(self.table, field.name().as_bytes());
        };
        // Asked again for every read: a write in between is seen like any other.
        let version = call.raw_fields_version(self.table)?;
        let known = known.at(self.table, version);
        if let Some(value) = known.find(field) {
            check(call, self.table, field, value)?;
            return Ok(value);
        }
        let value = call.raw_table_get_static(self.table, field.name().as_bytes())?;
        known.record(field, value);
        Ok(value)
    }
    /// Give the actor back what it knows. Without this the actor merely
    /// reads its table again.
    pub(super) fn finish(self, call: &mut HostCall<'_>) {
        if let Some(known) = self.known
            && let Some(actor) = call.store().actors.get_mut(&self.table)
        {
            actor.known = known;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_live_pose_owns_exactly_the_native_state_properties() {
        // The names the state read skipped during playback before fields had
        // identities. A field is native exactly when it is one of them.
        const OWNED: [&str; 39] = [
            "aux",
            "basezoom",
            "basezoom_x",
            "basezoom_y",
            "basezoom_z",
            "cropbottom",
            "cropleft",
            "cropright",
            "croptop",
            "depth_test",
            "depth_test_on_fail",
            "depth_write",
            "diffuse",
            "draw_by_z_position",
            "fadebottom",
            "fadeleft",
            "faderight",
            "fadetop",
            "glow",
            "halign",
            "mask_dest",
            "mask_source",
            "quaternion",
            "rot_x_deg",
            "rot_y_deg",
            "rot_z_deg",
            "skew_x",
            "skew_y",
            "texture_filtering",
            "texture_wrapping",
            "valign",
            "vertex_colors",
            "x",
            "y",
            "z",
            "zoom",
            "zoom_x",
            "zoom_y",
            "zoom_z",
        ];
        let mut seen = 0;
        for (name, native) in NAMES.iter().zip(NATIVE) {
            let owned = name
                .strip_prefix("__songlua_state_")
                .is_some_and(|property| OWNED.contains(&property));
            assert_eq!(native, owned, "{name}");
            seen += usize::from(owned);
        }
        assert_eq!(seen, OWNED.len(), "every owned property is a field");
    }

    #[test]
    fn answers_belong_to_one_table_at_one_version() {
        let mut known = Known::default();
        let mut vm = Vm::new();
        let (first, second) = (vm.try_new_table().unwrap(), vm.try_new_table().unwrap());
        assert_eq!(known.at(first, 0).find(Field(0)), None);
        known.at(first, 0).record(Field(0), Value::Number(1.0));
        known.at(first, 0).record(Field(70), Value::Nil);
        assert_eq!(known.at(first, 0).find(Field(0)), Some(Value::Number(1.0)));
        assert_eq!(known.at(first, 0).find(Field(70)), Some(Value::Nil));
        assert_eq!(known.at(first, 0).find(Field(1)), None);
        // Another version or another table knows nothing.
        assert_eq!(known.at(first, 1).find(Field(0)), None);
        known.at(first, 1).record(Field(0), Value::Boolean(true));
        assert_eq!(known.at(second, 1).find(Field(0)), None);
        assert_eq!(known.at(first, 1).find(Field(0)), None);
    }
}
