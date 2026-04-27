use super::super::*;

pub(in crate::screens::options) const SYNC_PACK_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::SyncPackPack,
        label: lookup_key("OptionsSyncPack", "SyncPackPack"),
        choices: &[localized_choice("OptionsSyncPack", "AllPacks")],
        inline: false,
    },
    SubRow {
        id: SubRowId::SyncPackStart,
        label: lookup_key("OptionsSyncPack", "SyncPackStart"),
        choices: &[localized_choice("Common", "Start")],
        inline: false,
    },
];

pub(in crate::screens::options) const SYNC_PACK_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::SpPack,
        name: lookup_key("OptionsSyncPack", "SyncPackPack"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSyncPackHelp",
            "SyncPackPackHelp",
        ))],
    },
    Item {
        id: ItemId::SpStart,
        name: lookup_key("OptionsSyncPack", "SyncPackStart"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSyncPackHelp",
            "SyncPackStartHelp",
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

