use super::main::push_display_modifier_rows;
use super::*;

pub(super) fn build_display_rows(
    noteskin_names: &[String],
    smx_bg_pack_names: &[String],
    smx_judge_pack_names: &[String],
    return_screen: Screen,
) -> RowMap {
    let mut b = RowBuilder::new();
    push_display_modifier_rows(&mut b, noteskin_names, smx_bg_pack_names, smx_judge_pack_names);
    b.push(
        Row::custom(
            RowId::WhatComesNext,
            lookup_key("PlayerOptions", "WhatComesNext"),
            lookup_key("PlayerOptionsHelp", "WhatComesNextAdvancedHelp"),
            super::WHAT_COMES_NEXT,
            what_comes_next_choices(OptionsPane::Display, return_screen),
        )
        .with_mirror_across_players(),
    );
    b.push(Row::exit());
    b.finish()
}
