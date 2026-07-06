use super::*;
use deadsync_config::update::{
    set_f32_if_changed, set_f64_if_changed, set_if_changed, set_pair_if_changed,
    set_quad_if_changed,
};

mod audio;
mod lights;
mod machine;
mod null_or_die;
mod system;
mod theme;

pub use self::audio::*;
pub use self::lights::*;
pub use self::machine::*;
pub use self::null_or_die::*;
pub use self::system::*;
pub use self::theme::*;

fn update_config_value<T>(value: T, field: impl FnOnce(&mut Config) -> &mut T) -> bool
where
    T: PartialEq,
{
    let changed = {
        let mut cfg = lock_config();
        set_if_changed(field(&mut cfg), value)
    };
    if changed {
        save_without_keymaps();
    }
    changed
}

fn update_config_f32(value: f32, field: impl FnOnce(&mut Config) -> &mut f32) -> bool {
    let changed = {
        let mut cfg = lock_config();
        set_f32_if_changed(field(&mut cfg), value)
    };
    if changed {
        save_without_keymaps();
    }
    changed
}

fn update_config_f64(value: f64, field: impl FnOnce(&mut Config) -> &mut f64) -> bool {
    let changed = {
        let mut cfg = lock_config();
        set_f64_if_changed(field(&mut cfg), value)
    };
    if changed {
        save_without_keymaps();
    }
    changed
}
