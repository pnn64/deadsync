//! Base scale and alignment are immediate properties, outside Actor's tween state.
#[cfg(any(test, feature = "legacy-oracle"))]
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
    #[cfg(any(test, feature = "legacy-oracle"))]
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

    #[cfg(any(test, feature = "legacy-oracle"))]
    pub fn zoom_axis(self, field: BaseField) -> f32 {
        // Preserve the table representation's distinction between an absent
        // axis and an explicitly authored 1.0 (including snapshot exports).
        self.get(field).or(self.get(BaseField::Zoom)).unwrap_or(1.0)
    }

    #[cfg(any(test, feature = "legacy-oracle"))]
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


