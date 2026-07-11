use deadsync_config::prelude::get;

pub use crate::resources::{machine_font_key, machine_font_key_for_text};
pub use deadsync_theme::FontRole;

#[inline]
pub fn current_machine_font_key(role: FontRole) -> &'static str {
    machine_font_key(get().machine_font, role)
}

#[inline]
pub fn current_machine_font_key_for_text(role: FontRole, text: &str) -> &'static str {
    machine_font_key_for_text(get().machine_font, role, text)
}
