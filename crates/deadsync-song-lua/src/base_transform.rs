//! Base scale and alignment are immediate properties, outside Actor's tween state.
use mlua::{Table, Value};

macro_rules! base_fields {
    ($($field:ident => $key:ident),* $(,)?) => {
        #[derive(Clone, Copy)]
        pub(crate) enum BaseField { $($field),* }

        impl BaseField {
            pub const ALL: [Self; 6] = [$(Self::$field),*];

            pub fn from_key(key: &str) -> Option<Self> {
                match key {
                    $(stringify!($key) => Some(Self::$field),)*
                    _ => None,
                }
            }

            pub fn state_key(self) -> &'static str {
                match self {
                    $(Self::$field => concat!("__songlua_state_", stringify!($key)),)*
                }
            }
        }
    };
}

base_fields! {
    Zoom => basezoom,
    ZoomX => basezoom_x,
    ZoomY => basezoom_y,
    ZoomZ => basezoom_z,
    HAlign => halign,
    VAlign => valign,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct BaseTransform {
    values: [f32; 6],
    present: u8,
}

impl BaseTransform {
    pub fn read(actor: &Table) -> mlua::Result<Self> {
        let mut state = Self::default();
        for field in BaseField::ALL {
            if let Some(value) = actor.raw_get::<Option<f32>>(field.state_key())? {
                state.set(field, value);
            }
        }
        Ok(state)
    }

    pub fn get(self, field: BaseField) -> Option<f32> {
        (self.present & (1 << field as u8) != 0).then_some(self.values[field as usize])
    }

    pub fn set(&mut self, field: BaseField, value: f32) {
        self.values[field as usize] = value;
        self.present |= 1 << field as u8;
    }

    pub fn zoom_axis(self, field: BaseField) -> f32 {
        // Preserve the table representation's distinction between an absent
        // axis and an explicitly authored 1.0 (including snapshot exports).
        self.get(field).or(self.get(BaseField::Zoom)).unwrap_or(1.0)
    }

    pub fn snapshot(self) -> impl Iterator<Item = (&'static str, Value)> {
        BaseField::ALL.into_iter().map(move |field| {
            (
                field.state_key(),
                self.get(field)
                    .map_or(Value::Nil, |v| Value::Number(v.into())),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_preserve_absence_explicit_one_and_float_bits() {
        let lua = mlua::Lua::new();
        let table = lua.create_table().unwrap();
        table.raw_set("__songlua_state_basezoom", 2.0).unwrap();
        let initial = BaseTransform::read(&table).unwrap();
        assert_eq!(initial.zoom_axis(BaseField::ZoomX), 2.0);
        assert_eq!(initial.get(BaseField::ZoomX), None);
        for value in [1.0, -0.0, f32::INFINITY, f32::NEG_INFINITY, f32::NAN, -1.25] {
            table.raw_set("__songlua_state_basezoom_x", value).unwrap();
            let native = BaseTransform::read(&table).unwrap();
            assert_eq!(
                native.zoom_axis(BaseField::ZoomX).to_bits(),
                value.to_bits()
            );
            let copy = lua.create_table().unwrap();
            for (key, value) in native.snapshot() {
                copy.raw_set(key, value).unwrap();
            }
            assert_eq!(
                copy.raw_get::<f32>("__songlua_state_basezoom_x")
                    .unwrap()
                    .to_bits(),
                value.to_bits()
            );
            assert!(
                copy.raw_get::<Value>("__songlua_state_basezoom_y")
                    .unwrap()
                    .is_nil()
            );
            assert!(
                copy.raw_get::<Value>("__songlua_state_halign")
                    .unwrap()
                    .is_nil()
            );
        }
        assert_eq!(std::mem::size_of::<BaseTransform>(), 28);
    }
}
