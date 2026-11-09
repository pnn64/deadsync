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
    pub fn get(self, field: BaseField) -> Option<f32> {
        (self.present & (1 << field as u8) != 0).then_some(self.values[field as usize])
    }

    pub fn set(&mut self, field: BaseField, value: f32) {
        self.values[field as usize] = value;
        self.present |= 1 << field as u8;
    }
}
