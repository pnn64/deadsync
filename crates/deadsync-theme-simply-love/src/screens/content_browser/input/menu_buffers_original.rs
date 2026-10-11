fn original_edit_query(state: &mut State, edit: impl FnOnce(&mut String)) -> bool {
    let before = state.query.clone();
    edit(&mut state.query);
    if state.query == before {
        return false;
    }
    state.query_idle = 0.0;
    state.caret_elapsed = 0.0;
    state.search_why.clear();
    rebuild_results(state, None);
    true
}

pub fn original_handle_raw_key_event(
    state: &mut State,
    key: Option<&RawKeyboardEvent>,
    text: Option<&str>,
    effects: &mut Vec<ThemeEffect>,
) -> bool {
    if let Some(key) = key {
        if !key.pressed {
            return false;
        }
        // A pack's page is read with the pad's actions, not typed into: Escape
        // there is BACK, out to the results it was opened from -- not a key
        // that clears the search those results came from. The reload question
        // is answered the same way.
        if state.zone == Zone::Detail
            || state.reload_prompt.is_some()
            || state.sync_dialog.is_some()
            || sync_dialog::overlay_visible(state)
        {
            return false;
        }
        if search_field_showing(state) {
            match key.code {
                KeyCode::Backspace => {
                    if original_edit_query(state, |query| {
                        query.pop();
                    }) {
                        effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
                    }
                    return true;
                }
                // The box is already live, so there is nothing to confirm --
                // Enter just puts the reader on the answer. Once they are on
                // it, Enter is START: it opens the pack under the cursor.
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    if state.zone == Zone::List {
                        return false;
                    }
                    state.zone = Zone::List;
                    state.cursor = 0;
                    return true;
                }
                KeyCode::Escape => {
                    if original_edit_query(state, String::clear) {
                        effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
                    }
                    return true;
                }
                _ => {}
            }
        }
        return match key.code {
            KeyCode::PageUp | KeyCode::PageDown => {
                state.nav_hold = None;
                let direction = if key.code == KeyCode::PageUp { -1 } else { 1 };
                if page(state, direction) == Outcome::Moved {
                    effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
                }
                true
            }
            // Escape clears a committed search before it leaves, so a query
            // costs one key to undo rather than a trip out through the menu.
            KeyCode::Escape => {
                if state.query.is_empty() {
                    false
                } else {
                    state.query.clear();
                    rebuild_results(state, None);
                    effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
                    true
                }
            }
            _ => false,
        };
    }

    let Some(text) = text else {
        return false;
    };
    let printable: String = text.chars().filter(|ch| !ch.is_control()).collect();
    if printable.is_empty() {
        return false;
    }
    // Reading a pack is not searching for one.
    if state.zone == Zone::Detail
        || state.removing.is_some()
        || state.sync_dialog.is_some()
        || sync_dialog::overlay_visible(state)
        || state.reload_prompt.is_some()
    {
        return false;
    }
    if !search_field_showing(state) {
        focus_search(state);
    }
    if original_edit_query(state, |query| {
        for ch in printable.chars() {
            if query.chars().count() >= QUERY_MAX_CHARS {
                break;
            }
            query.push(ch);
        }
    }) {
        effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
    }
    true
}
