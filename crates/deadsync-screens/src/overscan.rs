use deadsync_input::{InputEvent, VirtualAction};

pub const FIELD_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    AddHeight,
    AddWidth,
    TranslateX,
    TranslateY,
}

impl Field {
    #[inline(always)]
    pub const fn index(self) -> usize {
        match self {
            Self::AddHeight => 0,
            Self::AddWidth => 1,
            Self::TranslateX => 2,
            Self::TranslateY => 3,
        }
    }

    #[inline(always)]
    const fn from_index(index: usize) -> Self {
        match index {
            0 => Self::AddHeight,
            1 => Self::AddWidth,
            2 => Self::TranslateX,
            _ => Self::TranslateY,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Values {
    pub add_height: i32,
    pub add_width: i32,
    pub translate_x: i32,
    pub translate_y: i32,
}

impl Values {
    #[inline(always)]
    pub const fn get(self, field: Field) -> i32 {
        match field {
            Field::AddHeight => self.add_height,
            Field::AddWidth => self.add_width,
            Field::TranslateX => self.translate_x,
            Field::TranslateY => self.translate_y,
        }
    }

    #[inline(always)]
    fn get_mut(&mut self, field: Field) -> &mut i32 {
        match field {
            Field::AddHeight => &mut self.add_height,
            Field::AddWidth => &mut self.add_width,
            Field::TranslateX => &mut self.translate_x,
            Field::TranslateY => &mut self.translate_y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adjustment {
    pub field: Field,
    pub delta: i32,
}

impl Adjustment {
    #[inline(always)]
    pub const fn new(field: Field, delta: i32) -> Self {
        Self { field, delta }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Preview(Values),
    Commit(Values),
    Cancel(Values),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    values: Values,
    initial: Values,
    selected: Field,
}

impl State {
    pub const fn new(initial: Values) -> Self {
        Self {
            values: initial,
            initial,
            selected: Field::AddHeight,
        }
    }

    #[inline(always)]
    pub const fn values(&self) -> Values {
        self.values
    }

    #[inline(always)]
    pub const fn selected(&self) -> Field {
        self.selected
    }

    pub fn reset(&mut self, values: Values) {
        self.values = values;
        self.initial = values;
        self.selected = Field::AddHeight;
    }
}

pub fn apply_adjustment(state: &mut State, adjustment: Adjustment) -> Values {
    state.selected = adjustment.field;
    let value = state.values.get_mut(adjustment.field);
    *value = value.saturating_add(adjustment.delta);
    state.values
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> Action {
    if !ev.pressed {
        return Action::None;
    }
    match ev.action {
        VirtualAction::p1_start | VirtualAction::p2_start => Action::Commit(state.values),
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.values = state.initial;
            Action::Cancel(state.values)
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
            let selected = (state.selected.index() + FIELD_COUNT - 1) % FIELD_COUNT;
            state.selected = Field::from_index(selected);
            Action::None
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
            let selected = (state.selected.index() + 1) % FIELD_COUNT;
            state.selected = Field::from_index(selected);
            Action::None
        }
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
            let selected = state.selected;
            Action::Preview(apply_adjustment(state, Adjustment::new(selected, -1)))
        }
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            let selected = state.selected;
            Action::Preview(apply_adjustment(state, Adjustment::new(selected, 1)))
        }
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use std::time::Instant;

    fn input(action: VirtualAction, pressed: bool) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    #[test]
    fn navigation_wraps_and_adjusts_the_selected_field() {
        let mut state = State::new(Values::default());

        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p1_up, true)),
            Action::None
        );
        assert_eq!(state.selected(), Field::TranslateY);
        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p1_right, true)),
            Action::Preview(Values {
                translate_y: 1,
                ..Values::default()
            })
        );

        for _ in 0..4 {
            let _ = handle_input(&mut state, &input(VirtualAction::p1_down, true));
        }
        assert_eq!(state.selected(), Field::TranslateY);
    }

    #[test]
    fn cancel_restores_entry_values_and_commit_keeps_working_values() {
        let initial = Values {
            add_height: 3,
            add_width: 4,
            translate_x: 5,
            translate_y: 6,
        };
        let mut state = State::new(initial);
        let _ = apply_adjustment(&mut state, Adjustment::new(Field::AddHeight, 7));

        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p1_start, true)),
            Action::Commit(Values {
                add_height: 10,
                ..initial
            })
        );
        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p1_back, true)),
            Action::Cancel(initial)
        );
        assert_eq!(state.values(), initial);
    }

    #[test]
    fn adjustments_saturate_and_released_input_does_nothing() {
        let mut state = State::new(Values {
            add_height: i32::MAX,
            ..Values::default()
        });
        assert_eq!(
            apply_adjustment(&mut state, Adjustment::new(Field::AddHeight, 1)).add_height,
            i32::MAX
        );
        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p1_left, false)),
            Action::None
        );
        assert_eq!(state.values().add_height, i32::MAX);
    }

    #[test]
    fn reset_replaces_working_and_cancel_values() {
        let mut state = State::new(Values::default());
        let next = Values {
            translate_x: -8,
            translate_y: 6,
            add_width: 14,
            add_height: -2,
        };

        state.reset(next);
        let _ = apply_adjustment(&mut state, Adjustment::new(Field::AddWidth, 5));
        assert_eq!(
            handle_input(&mut state, &input(VirtualAction::p2_back, true)),
            Action::Cancel(next)
        );
        assert_eq!(state.selected(), Field::AddWidth);
    }
}
