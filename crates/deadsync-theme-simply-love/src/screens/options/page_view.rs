use super::*;
use deadsync_config::options::{
    auto_screenshot_bit_from_choice, groovestats_scorebox_bit_from_choice,
    select_music_chart_info_bit_from_choice,
};
use deadsync_theme::views::{
    SystemOptionsPage, SystemOptionsPageKind, SystemOptionsPhase, SystemOptionsRow,
};

pub(super) fn push_page(
    actors: &mut Vec<Actor>,
    state: &State,
    assets: &AssetManager,
    draw: &mut dyn FnMut(&mut Vec<Actor>, SystemOptionsPage<'_>),
    screen_phase: SystemOptionsPhase,
) {
    let elapsed = || state.submenu_fade_t * state.page_transition_seconds();
    let phase = match state.submenu_transition {
        SubmenuTransition::FadeOutToSubmenu | SubmenuTransition::FadeOutToMain => {
            SystemOptionsPhase::Leaving(elapsed())
        }
        SubmenuTransition::FadeInSubmenu | SubmenuTransition::FadeInMain => {
            SystemOptionsPhase::Entering(elapsed())
        }
        SubmenuTransition::None => screen_phase,
    };
    match state.view {
        OptionsView::Main => {
            let rows = visible_items(state);
            let row_count = rows.len().saturating_sub(1);
            let selected = state.selected.min(row_count);
            let (description, detail) = page_help(state, rows.get(selected).copied());
            draw(
                actors,
                SystemOptionsPage {
                    id: 0,
                    title: "OPTIONS",
                    kind: SystemOptionsPageKind::ServiceMenu,
                    selected,
                    row_count,
                    row: &|index| {
                        let item = rows.get(..row_count)?.get(index)?;
                        Some(SystemOptionsRow {
                            label: item.name.get(),
                            choices: None,
                            selected_choice: 0,
                            inline: false,
                            disabled: false,
                            enabled_choices: None,
                            choice_tint: None,
                        })
                    },
                    description,
                    detail,
                    phase,
                },
            );
        }
        OptionsView::Submenu(kind) => {
            let rows = submenu_rows(kind);
            let items = submenu_items(kind);
            let visible_rows = cached_submenu_visible_rows(state, kind);
            let row_count = visible_rows.len();
            let actual_index = |index| visible_rows.get(index).copied();
            let selected = state.sub_selected.min(row_count);
            let selected_item = actual_index(selected)
                .and_then(|index| items.get(index))
                .or_else(|| items.last());
            let (description, detail) = page_help(state, selected_item);
            let choices = submenu_choice_indices(state, kind);
            draw(
                actors,
                SystemOptionsPage {
                    id: kind as usize + 1,
                    title: submenu_title(state, kind),
                    kind: if kind == SubmenuKind::GraphicsBasic {
                        SystemOptionsPageKind::BasicGraphics
                    } else {
                        SystemOptionsPageKind::Settings
                    },
                    selected,
                    row_count,
                    row: &|index| {
                        let index = actual_index(index)?;
                        let row = rows.get(index)?;
                        let values = borrow_submenu_row_layout(state, assets, kind, index)
                            .map(|layout| Arc::clone(&layout.texts));
                        let count = values.as_ref().map_or(0, |values| values.len());
                        let selected_choice = choices
                            .get(index)
                            .copied()
                            .unwrap_or(0)
                            .min(count.saturating_sub(1));
                        Some(SystemOptionsRow {
                            label: row.label.get(),
                            choices: values,
                            selected_choice,
                            inline: row.inline,
                            disabled: is_submenu_row_disabled(state, kind, row.id),
                            enabled_choices: enabled_choice_bits(state, kind, row.id, count),
                            choice_tint: (row.id == SubRowId::PreferredColor)
                                .then(|| color::decorative_rgba(selected_choice as i32)),
                        })
                    },
                    description,
                    detail,
                    phase,
                },
            );
        }
    }
}

fn page_help(state: &State, item: Option<&Item>) -> (Option<Arc<str>>, Option<Arc<str>>) {
    let Some(item) = item else {
        return (None, None);
    };
    let description = item.help.iter().find_map(|entry| match entry {
        HelpEntry::Paragraph(text) => Some(text.get()),
        _ => None,
    });
    let detail = item.help.iter().find_map(|entry| match entry {
        HelpEntry::AppPath(kind) => Some(Arc::clone(&state.app_paths.get(*kind).display)),
        HelpEntry::SmxAssignmentStatus => Some(Arc::clone(&state.smx_assignment_status)),
        _ => None,
    });
    (description, detail)
}

pub(super) fn enabled_choice_bits(
    state: &State,
    kind: SubmenuKind,
    row: SubRowId,
    count: usize,
) -> Option<u64> {
    let (mask, bit): (_, fn(usize) -> u8) = match (kind, row) {
        (SubmenuKind::SelectMusic, SubRowId::ChartInfo) => (
            select_music_chart_info_enabled_mask(state),
            select_music_chart_info_bit_from_choice,
        ),
        (SubmenuKind::GrooveStats, SubRowId::GrooveStatsScoreboxBoards) => (
            groovestats_scorebox_enabled_mask(state),
            groovestats_scorebox_bit_from_choice,
        ),
        (SubmenuKind::Gameplay, SubRowId::AutoScreenshot) => (
            auto_screenshot_enabled_mask(state),
            auto_screenshot_bit_from_choice,
        ),
        _ => return None,
    };
    Some((0..count.min(u64::BITS as usize)).fold(0, |bits, choice| {
        bits | (u64::from(mask & bit(choice) != 0) << choice)
    }))
}
