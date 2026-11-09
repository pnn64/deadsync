//! Immediate render properties: these never belong to a tween destination.
use crate::SongLuaOverlayState;

macro_rules! render_flags {
    ($($flag:ident => $field:ident),* $(,)?) => {
        #[derive(Clone, Copy)]
        pub(crate) enum RenderFlag { $($flag),* }

        impl RenderFlag {
            pub const ALL: [Self; 8] = [$(Self::$flag),*];

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
}
