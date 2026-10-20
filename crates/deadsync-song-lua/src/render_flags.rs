//! Immediate render properties: these never belong to a tween destination.
use crate::SongLuaOverlayState;

macro_rules! render_flags {
    ($($flag:ident => $field:ident),* $(,)?) => {
        #[derive(Clone, Copy)]
        pub(crate) enum RenderFlag { $($flag),* }

        impl RenderFlag {
            pub const ALL: [Self; 8] = [$(Self::$flag),*];

            pub fn from_key(key: &str) -> Option<Self> {
                match key {
                    $(stringify!($field) => Some(Self::$flag),)*
                    _ => None,
                }
            }

            pub fn state_key(self) -> &'static str {
                match self {
                    $(Self::$flag => concat!("__songlua_state_", stringify!($field)),)*
                }
            }
        }

        #[derive(Clone, Copy, Default)]
        pub(crate) struct RenderFlags(u8);

        impl From<SongLuaOverlayState> for RenderFlags {
            fn from(state: SongLuaOverlayState) -> Self {
                let mut flags = Self::default();
                $(flags.set(RenderFlag::$flag, state.$field);)*
                flags
            }
        }
    };
}

render_flags! {
    DrawByZPosition => draw_by_z_position,
    MaskSource => mask_source,
    MaskDest => mask_dest,
    DepthTest => depth_test,
    DepthWrite => depth_write,
    DepthTestOnFail => depth_test_on_fail,
    TextureFiltering => texture_filtering,
    TextureWrapping => texture_wrapping,
}

impl RenderFlags {
    pub fn get(self, flag: RenderFlag) -> bool {
        self.0 & (1 << flag as u8) != 0
    }

    pub fn set(&mut self, flag: RenderFlag, value: bool) {
        let mask = 1 << flag as u8;
        self.0 = (self.0 & !mask) | if value { mask } else { 0 };
    }

    pub fn preserve_on_reset(&mut self, previous: Self) {
        // These properties were not reset by the existing InitState path.
        // ActorFrame's sorting policy is separate from Actor::InitState.
        for flag in [
            RenderFlag::DrawByZPosition,
            RenderFlag::MaskSource,
            RenderFlag::MaskDest,
        ] {
            self.set(flag, previous.get(flag));
        }
    }

    pub fn snapshot(self) -> impl Iterator<Item = (&'static str, mlua::Value)> {
        RenderFlag::ALL
            .into_iter()
            .map(move |flag| (flag.state_key(), mlua::Value::Boolean(self.get(flag))))
    }
}
