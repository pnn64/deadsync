use super::super::*;

pub(in crate::screens::options) const TOURNAMENT_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::TournamentMode,
        label: lookup_key("OptionsTournament", "EnableTournamentMode"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::TournamentScoring,
        label: lookup_key("OptionsTournament", "ScoringSystem"),
        choices: &[literal_choice("EX"), literal_choice("ITG")],
        inline: true,
    },
    SubRow {
        id: SubRowId::TournamentStepStats,
        label: lookup_key("OptionsTournament", "StepStats"),
        choices: &[
            localized_choice("OptionsTournament", "Hide"),
            localized_choice("OptionsTournament", "Show"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::TournamentEnforceNoCmod,
        label: lookup_key("OptionsTournament", "EnforceNoCmod"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
];

pub(in crate::screens::options) const TOURNAMENT_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::TmEnable,
        name: lookup_key("OptionsTournament", "EnableTournamentMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsTournamentHelp",
            "EnableTournamentModeHelp",
        ))],
    },
    Item {
        id: ItemId::TmScoring,
        name: lookup_key("OptionsTournament", "ScoringSystem"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsTournamentHelp",
            "ScoringSystemHelp",
        ))],
    },
    Item {
        id: ItemId::TmStepStats,
        name: lookup_key("OptionsTournament", "StepStats"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsTournamentHelp",
            "StepStatsHelp",
        ))],
    },
    Item {
        id: ItemId::TmEnforceNoCmod,
        name: lookup_key("OptionsTournament", "EnforceNoCmod"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsTournamentHelp",
            "EnforceNoCmodHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];
