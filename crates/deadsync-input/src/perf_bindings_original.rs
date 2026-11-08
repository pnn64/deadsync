// Frozen from 83fbce5457d789be942be4059eef4fdbb3191c34; test-only baseline.

#[must_use]
pub fn default_keymap() -> Keymap {
    use VirtualAction as A;
    let mut km = Keymap::default();
    // Player 1 defaults (Pump-standard QESZC + arrows, Enter/Escape).
    km.bind(
        A::p1_up,
        &[
            InputBinding::Key(KeyCode::ArrowUp),
            InputBinding::Key(KeyCode::KeyE),
        ],
    );
    km.bind(
        A::p1_down,
        &[
            InputBinding::Key(KeyCode::ArrowDown),
            InputBinding::Key(KeyCode::KeyQ),
        ],
    );
    km.bind(
        A::p1_left,
        &[
            InputBinding::Key(KeyCode::ArrowLeft),
            InputBinding::Key(KeyCode::KeyZ),
        ],
    );
    km.bind(
        A::p1_right,
        &[
            InputBinding::Key(KeyCode::ArrowRight),
            InputBinding::Key(KeyCode::KeyC),
        ],
    );
    km.bind(
        A::p1_center,
        &[
            InputBinding::Key(KeyCode::Space),
            InputBinding::Key(KeyCode::KeyS),
        ],
    );
    km.bind(A::p1_select, &[InputBinding::Key(KeyCode::Slash)]);
    km.bind(A::p1_start, &[InputBinding::Key(KeyCode::Enter)]);
    km.bind(A::p1_back, &[InputBinding::Key(KeyCode::Escape)]);
    // Player 2 defaults (numpad directions + Start on NumpadEnter).
    km.bind(A::p2_up, &[InputBinding::Key(KeyCode::Numpad8)]);
    km.bind(A::p2_down, &[InputBinding::Key(KeyCode::Numpad2)]);
    km.bind(A::p2_left, &[InputBinding::Key(KeyCode::Numpad4)]);
    km.bind(A::p2_right, &[InputBinding::Key(KeyCode::Numpad6)]);
    km.bind(A::p2_center, &[InputBinding::Key(KeyCode::Numpad5)]);
    km.bind(A::p2_select, &[InputBinding::Key(KeyCode::NumpadDecimal)]);
    km.bind(A::p2_start, &[InputBinding::Key(KeyCode::NumpadEnter)]);
    km.bind(A::p2_back, &[InputBinding::Key(KeyCode::Numpad0)]);
    km.bind(A::p1_operator, &[InputBinding::Key(KeyCode::ScrollLock)]);
    km.bind(A::p1_coin, &[InputBinding::Key(KeyCode::F1)]);
    km.bind(A::system_fast_forward, &[InputBinding::Key(KeyCode::Tab)]);
    km.bind(
        A::system_slow_down,
        &[InputBinding::Key(KeyCode::Backquote)],
    );
    // Leave dedicated menu buttons, P2 operator/coin, and restart unbound by default for now.
    km
}

pub fn load_keymap_from_ini_entries<'a, I>(section: Option<I>) -> Keymap
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    // When [Keymaps] is present, start from explicit user entries and then fill
    // in any completely missing actions from built-in defaults. When the whole
    // section is absent, fall back to defaults entirely.
    let Some(section) = section else {
        return default_keymap();
    };

    let mut km = Keymap::default();
    let mut seen = 0u32;

    for (key, value) in section {
        if let Some(action) = crate::action_from_ini_key(key) {
            let mut bindings = Vec::new();
            for tok in value.split(',') {
                if let Some(binding) = parse_binding_token(tok) {
                    bindings.push(binding);
                }
            }
            km.bind(action, &bindings);
            seen |= action.bit();
        }
    }

    let defaults = default_keymap();
    for action in ALL_VIRTUAL_ACTIONS {
        if seen & action.bit() != 0 {
            continue;
        }
        let mut bindings = Vec::new();
        let mut i = 0;
        while let Some(binding) = defaults.binding_at(action, i) {
            bindings.push(binding);
            i += 1;
        }
        if !bindings.is_empty() {
            km.bind(action, &bindings);
        }
    }
    restore_available_default_bindings(&mut km);

    km
}

#[inline(always)]
#[must_use]
pub const fn default_key_for_action(action: VirtualAction) -> Option<KeyCode> {
    use VirtualAction as A;
    match action {
        A::p1_up => Some(KeyCode::ArrowUp),
        A::p1_down => Some(KeyCode::ArrowDown),
        A::p1_left => Some(KeyCode::ArrowLeft),
        A::p1_right => Some(KeyCode::ArrowRight),
        A::p1_center => Some(KeyCode::Space),
        A::p1_select => Some(KeyCode::Slash),
        A::p1_start => Some(KeyCode::Enter),
        A::p1_back => Some(KeyCode::Escape),
        A::p1_operator => Some(KeyCode::ScrollLock),
        A::p1_coin => Some(KeyCode::F1),
        A::p2_up => Some(KeyCode::Numpad8),
        A::p2_down => Some(KeyCode::Numpad2),
        A::p2_left => Some(KeyCode::Numpad4),
        A::p2_right => Some(KeyCode::Numpad6),
        A::p2_center => Some(KeyCode::Numpad5),
        A::p2_select => Some(KeyCode::NumpadDecimal),
        A::p2_start => Some(KeyCode::NumpadEnter),
        A::p2_back => Some(KeyCode::Numpad0),
        // System (non-player) tier: Tab acceleration fast-forward / slow-down.
        A::system_fast_forward => Some(KeyCode::Tab),
        A::system_slow_down => Some(KeyCode::Backquote),
        _ => None,
    }
}

#[inline(always)]
pub fn default_binding_for_action(action: VirtualAction) -> Option<InputBinding> {
    default_key_for_action(action).map(InputBinding::Key)
}

#[inline(always)]
fn bindings_start_with_default(action: VirtualAction, bindings: &[InputBinding]) -> bool {
    matches!(
        (default_binding_for_action(action), bindings.first()),
        (Some(default_binding), Some(first_binding)) if default_binding == *first_binding
    )
}

#[inline(always)]
fn first_editable_binding_slot(action: VirtualAction, bindings: &[InputBinding]) -> usize {
    if bindings_start_with_default(action, bindings) {
        1
    } else {
        0
    }
}

#[inline(always)]
const fn requested_to_actual_binding_slot(requested_index: usize, first_editable: usize) -> usize {
    if first_editable == 0 {
        requested_index.saturating_sub(1)
    } else {
        requested_index
    }
}

#[inline(always)]
fn load_action_bindings(keymap: &Keymap, action: VirtualAction) -> Vec<InputBinding> {
    let mut bindings = Vec::new();
    let mut i = 0;
    while let Some(binding) = keymap.binding_at(action, i) {
        bindings.push(binding);
        i += 1;
    }
    bindings
}

#[inline(always)]
fn remove_matching_keyboard_binding(
    bindings: &mut Vec<InputBinding>,
    keycode: KeyCode,
    keep_first: bool,
) {
    let mut slot = 0;
    bindings.retain(|binding| {
        let keep = (keep_first && slot == 0)
            || !matches!(binding, InputBinding::Key(code) if *code == keycode);
        slot += 1;
        keep
    });
}

#[inline(always)]
fn remove_matching_input_binding(bindings: &mut Vec<InputBinding>, binding: InputBinding) {
    bindings.retain(|existing| *existing != binding);
}

#[inline(always)]
fn keymap_contains_binding(keymap: &Keymap, binding: InputBinding) -> bool {
    if let InputBinding::Key(code) = binding {
        return keymap.keycode_mapped(code);
    }
    ALL_VIRTUAL_ACTIONS
        .iter()
        .any(|&action| keymap.bindings_for_action(action).contains(&binding))
}

#[inline(always)]
pub fn restore_available_default_bindings(keymap: &mut Keymap) {
    let mut scratch = Vec::new();
    for act in ALL_VIRTUAL_ACTIONS {
        let Some(default_binding) = default_binding_for_action(act) else {
            continue;
        };
        let bindings = keymap.bindings_for_action(act);
        let slot = bindings
            .iter()
            .position(|binding| *binding == default_binding);
        if slot == Some(0) || (slot.is_none() && keymap_contains_binding(keymap, default_binding)) {
            continue;
        }
        // Only actions that change need owned scratch; share it across repairs.
        scratch.clear();
        scratch.reserve(bindings.len() + usize::from(slot.is_none()));
        scratch.extend_from_slice(bindings);
        if let Some(slot) = slot {
            scratch.remove(slot);
        }
        scratch.insert(0, default_binding);
        keymap.bind(act, &scratch);
    }
}

#[inline(always)]
fn set_binding_at_slot(bindings: &mut Vec<InputBinding>, slot_index: usize, binding: InputBinding) {
    let slot_index = slot_index.min(bindings.len());
    if bindings.len() <= slot_index {
        bindings.push(binding);
    } else {
        bindings[slot_index] = binding;
    }
}

#[must_use]
pub fn updated_keymap_unique_keyboard(
    current: &Keymap,
    action: VirtualAction,
    index: usize,
    keycode: KeyCode,
) -> Keymap {
    let mut new_map = Keymap::default();

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings = load_action_bindings(current, act);
        let binding = InputBinding::Key(keycode);
        let keep_default = act == action
            && bindings_start_with_default(act, &bindings)
            && bindings.first() == Some(&binding);

        // Remove this key from every slot so one physical key cannot fan out
        // to multiple actions.
        remove_matching_keyboard_binding(&mut bindings, keycode, keep_default);

        if act == action {
            let first_editable = first_editable_binding_slot(act, &bindings);
            let mut effective_index = requested_to_actual_binding_slot(index, first_editable);
            // If Secondary requested but there is no Primary yet, collapse to
            // the first editable slot.
            if effective_index > first_editable && bindings.len() <= first_editable {
                effective_index = first_editable;
            }
            if keep_default {
                if effective_index >= first_editable && effective_index < bindings.len() {
                    bindings.remove(effective_index);
                }
            } else {
                set_binding_at_slot(&mut bindings, effective_index, binding);
            }
        }

        new_map.bind(act, &bindings);
    }

    restore_available_default_bindings(&mut new_map);
    new_map
}

#[must_use]
pub fn updated_keymap_unique_gamepad(
    current: &Keymap,
    action: VirtualAction,
    index: usize,
    binding: InputBinding,
) -> Keymap {
    let mut new_map = Keymap::default();

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings = load_action_bindings(current, act);

        // Remove this binding from every slot so one physical control cannot
        // remain assigned elsewhere.
        remove_matching_input_binding(&mut bindings, binding);

        if act == action {
            let first_editable = first_editable_binding_slot(act, &bindings);
            let mut effective_index = requested_to_actual_binding_slot(index, first_editable);
            // If Secondary requested but there is no Primary yet, collapse to
            // the first editable slot.
            if effective_index > first_editable && bindings.len() <= first_editable {
                effective_index = first_editable;
            }
            set_binding_at_slot(&mut bindings, effective_index, binding);
        }

        new_map.bind(act, &bindings);
    }

    restore_available_default_bindings(&mut new_map);
    new_map
}

#[must_use]
pub fn cleared_keymap(current: &Keymap, action: VirtualAction, index: usize) -> (Keymap, bool) {
    let mut new_map = Keymap::default();
    let mut changed = false;

    for act in ALL_VIRTUAL_ACTIONS {
        let mut bindings = load_action_bindings(current, act);
        if act == action {
            let first_editable = first_editable_binding_slot(act, &bindings);
            let effective_index = requested_to_actual_binding_slot(index, first_editable);
            if effective_index < bindings.len() {
                bindings.remove(effective_index);
                changed = true;
            }
        }
        new_map.bind(act, &bindings);
    }

    if changed {
        restore_available_default_bindings(&mut new_map);
    }
    (new_map, changed)
}
