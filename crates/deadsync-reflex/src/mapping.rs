use super::BUTTON_CODE_BASE;
use deadlib_platform::input::REFLEX_PAD_ID_BASE;
use deadsync_input::{
    ALL_VIRTUAL_ACTIONS, GamepadCodeBinding, InputBinding, Keymap, VirtualAction,
};

/// First-use defaults are ordinary editable mappings. Existing RE:Flex mappings
/// are authoritative, including partial custom mappings. Other devices and keys
/// are preserved. Called once per player slot, not on every reconnect.
pub fn install_default_bindings(player: usize, keymap: &mut Keymap) -> bool {
    if player >= 2 {
        return false;
    }
    let device = REFLEX_PAD_ID_BASE + player;
    for action in ALL_VIRTUAL_ACTIONS {
        for i in 0.. {
            let Some(binding) = keymap.binding_at(action, i) else {
                break;
            };
            if let InputBinding::GamepadCode(b) = binding
                && (BUTTON_CODE_BASE..BUTTON_CODE_BASE + 4).contains(&b.code_u32)
                && b.device.is_none_or(|d| d == device)
            {
                return false;
            }
        }
    }
    let actions = if player == 0 {
        [
            VirtualAction::p1_left,
            VirtualAction::p1_down,
            VirtualAction::p1_up,
            VirtualAction::p1_right,
        ]
    } else {
        [
            VirtualAction::p2_left,
            VirtualAction::p2_down,
            VirtualAction::p2_up,
            VirtualAction::p2_right,
        ]
    };
    for (panel, action) in actions.into_iter().enumerate() {
        let mut bindings = (0..)
            .map_while(|i| keymap.binding_at(action, i))
            .collect::<Vec<_>>();
        bindings.push(InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: BUTTON_CODE_BASE + panel as u32,
            device: Some(device),
            uuid: None,
        }));
        keymap.bind(action, &bindings);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_platform::input::{PadCode, PadEvent, PadId};
    #[test]
    fn defaults_preserve_keys_and_route_two_pads_without_native_button_collisions() {
        let mut km = deadsync_input::default_keymap();
        let key = km.binding_at(VirtualAction::p1_left, 0);
        assert!(install_default_bindings(0, &mut km));
        assert!(install_default_bindings(1, &mut km));
        assert_eq!(km.binding_at(VirtualAction::p1_left, 0), key);
        assert!(!install_default_bindings(0, &mut km));
        let mut state = deadsync_input::keymap::InputState::new(&km, 0.0);
        let now = std::time::Instant::now();
        for (player, expected) in [(0, VirtualAction::p1_left), (1, VirtualAction::p2_left)] {
            let event = PadEvent::RawButton {
                id: PadId((REFLEX_PAD_ID_BASE + player) as u32),
                code: PadCode(BUTTON_CODE_BASE),
                uuid: [1; 16],
                pressed: true,
                value: 1.0,
                timestamp: now,
                host_nanos: 0,
            };
            let actions: Vec<_> = state.map_pad(&event, || now).map(|e| e.action).collect();
            assert!(actions.contains(&expected));
            let native = PadEvent::RawButton {
                id: PadId(0),
                code: PadCode(0),
                uuid: [2; 16],
                pressed: true,
                value: 1.0,
                timestamp: now,
                host_nanos: 0,
            };
            assert_eq!(state.map_pad(&native, || now).count(), 0);
        }
    }
}
