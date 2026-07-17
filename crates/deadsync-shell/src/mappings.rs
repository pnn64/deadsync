use deadsync_config::prelude as config;
use deadsync_input::{any_player_has_dedicated_menu_buttons_for_mode, get_keymap};
use deadsync_theme_simply_love::SimplyLoveMappingsConfigRequest;
use deadsync_theme_simply_love::views::MappingsRuntimeView;

pub(crate) fn runtime_view() -> MappingsRuntimeView {
    let cfg = config::get();
    MappingsRuntimeView {
        keymap: get_keymap(),
        machine_font: cfg.machine_font,
        input_debounce_seconds: cfg.input_debounce_seconds,
        dedicated_three_key_nav: cfg.three_key_navigation && cfg.only_dedicated_menu_buttons,
    }
}

pub(crate) fn execute(request: SimplyLoveMappingsConfigRequest) {
    use SimplyLoveMappingsConfigRequest as Request;

    let check_dedicated = match request {
        Request::BindKeyboard {
            action,
            index,
            code,
        } => {
            config::update_keymap_binding_unique_keyboard(action, index, code);
            true
        }
        Request::BindGamepad {
            action,
            index,
            binding,
        } => {
            config::update_keymap_binding_unique_gamepad(action, index, binding);
            true
        }
        Request::Clear { action, index } => config::clear_keymap_binding(action, index),
    };

    if check_dedicated {
        disable_unsupported_dedicated_nav();
    }
}

fn disable_unsupported_dedicated_nav() {
    let cfg = config::get();
    if cfg.only_dedicated_menu_buttons
        && !any_player_has_dedicated_menu_buttons_for_mode(cfg.three_key_navigation)
    {
        config::update_only_dedicated_menu_buttons(false);
    }
}
