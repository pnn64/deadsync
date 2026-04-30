use super::super::*;

pub(in crate::screens::options) const COURSE_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::ShowRandomCourses,
        label: lookup_key("OptionsCourse", "ShowRandomCourses"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowMostPlayed,
        label: lookup_key("OptionsCourse", "ShowMostPlayed"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowIndividualScores,
        label: lookup_key("OptionsCourse", "ShowIndividualScores"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::AutosubmitIndividual,
        label: lookup_key("OptionsCourse", "AutosubmitIndividual"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
];

pub(in crate::screens::options) const COURSE_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::CrsShowRandom,
        name: lookup_key("OptionsCourse", "ShowRandomCourses"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCourseHelp",
            "ShowRandomCoursesHelp",
        ))],
    },
    Item {
        id: ItemId::CrsShowMostPlayed,
        name: lookup_key("OptionsCourse", "ShowMostPlayed"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCourseHelp",
            "ShowMostPlayedHelp",
        ))],
    },
    Item {
        id: ItemId::CrsShowIndividualScores,
        name: lookup_key("OptionsCourse", "ShowIndividualScores"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCourseHelp",
            "ShowIndividualScoresHelp",
        ))],
    },
    Item {
        id: ItemId::CrsAutosubmitIndividual,
        name: lookup_key("OptionsCourse", "AutosubmitIndividual"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCourseHelp",
            "AutosubmitIndividualHelp",
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
