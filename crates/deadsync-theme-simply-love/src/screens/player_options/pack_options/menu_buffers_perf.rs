use super::*;
use crate::menu_buffers_perf_support::compare;
use deadsync_noteskin::pack::Choice;
use std::hint::black_box;

include!("menu_buffers_original.rs");

fn rows(names: Vec<String>) -> RowMap {
    let mut rows = RowMap::new();
    rows.insert(Row::custom(
        RowId::NoteSkin,
        lookup_key("PlayerOptions", "NoteSkin"),
        lookup_key("PlayerOptionsHelp", "NoteSkin"),
        CustomBinding { apply: apply_part },
        names,
    ));
    rows.display_order.push(RowId::NoteSkin);
    rows
}

fn menu(skins: &[Skin]) -> PackMenu {
    let mut menu = PackMenu::new(&[]);
    menu.skins = skins.to_vec();
    menu
}

fn fixture(count: usize, variants: usize) -> (Vec<Skin>, Vec<String>) {
    let mut names: Vec<String> = ["default", "cel", "metal", "日本語", "cel"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let skins = (0..count)
        .map(|n| {
            let id = if n == 0 {
                "cel-workshop".to_owned()
            } else {
                format!("skin-{n}")
            };
            names.push(id.clone());
            Skin {
                id,
                base: "cel".into(),
                preview: "preview.png".into(),
                options: SLOTS
                    .iter()
                    .flat_map(|slot| {
                        (0..variants).map(move |n| Choice {
                            slot: (*slot).into(),
                            id: if n == 0 {
                                "base".into()
                            } else {
                                format!("variant-{n}")
                            },
                            label: format!("Choice {n} 日本語"),
                            cell: n as u16,
                            files: vec![],
                            metrics: vec![],
                        })
                    })
                    .collect(),
            }
        })
        .collect();
    (skins, names)
}

fn assert_rows(a: &RowMap, b: &RowMap, ma: &PackMenu, mb: &PackMenu) {
    assert_eq!(a.display_order, b.display_order);
    assert_eq!(ma.choices, mb.choices);
    for (a, b) in a.rows.iter().zip(&b.rows) {
        match (a, b) {
            (None, None) => {}
            (Some(a), Some(b)) => {
                assert_eq!(a.id, b.id);
                assert_eq!((a.name.section, a.name.key), (b.name.section, b.name.key));
                assert_eq!(a.selected_choice_index, b.selected_choice_index);
                assert_eq!(
                    a.choices
                        .iter()
                        .map(TextContent::as_str)
                        .collect::<Vec<_>>(),
                    b.choices
                        .iter()
                        .map(TextContent::as_str)
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    a.help
                        .iter()
                        .map(|h| (&h.text, h.char_count))
                        .collect::<Vec<_>>(),
                    b.help
                        .iter()
                        .map(|h| (&h.text, h.char_count))
                        .collect::<Vec<_>>()
                );
                assert_eq!(a.choice_difficulty_indices, b.choice_difficulty_indices);
                assert_eq!(a.mirror_across_players, b.mirror_across_players);
                assert_eq!(a.choice_widths, b.choice_widths);
                assert_eq!(a.choice_offsets, b.choice_offsets);
                assert_eq!(a.choice_height, b.choice_height);
            }
            _ => panic!("row presence differs"),
        }
    }
}

#[test]
fn menu_buffers_pack_rows_preserve_choices_order_and_missing_selections() {
    crate::i18n::init_for_tests();
    for count in [0, 1, 4] {
        let (mut skins, mut names) = fixture(count, 3);
        if let Some(skin) = skins.first().cloned() {
            skins.push(skin);
        }
        names.push("unavailable-skin".into());
        let mut players = [PlayerOptionsData::default(), PlayerOptionsData::default()];
        players[0].arrow_noteskin = Some(NoteSkin::new("cel-workshop?arrows=variant-2"));
        players[0].mine_noteskin = Some(NoteSkin::new("absent?mines=old"));
        players[1].arrow_noteskin = Some(NoteSkin::new("cel-workshop?arrows=lost"));
        players[1].tap_explosion_noteskin = Some(NoteSkin::none_choice());
        let (mut a, mut b) = (rows(names.clone()), rows(names));
        let (mut ma, mut mb) = (menu(&skins), menu(&skins));
        for _ in 0..3 {
            ma.original_add_rows(&mut a, &players);
            mb.add_rows(&mut b, &players);
            assert_rows(&a, &b, &ma, &mb);
        }
        let sizes = &b.row(RowId::SkinMineSize).choices;
        assert_eq!(sizes.len(), 191);
        assert_eq!(sizes[0].as_str(), "10%");
        assert_eq!(sizes[190].as_str(), "200%");
        assert!(sizes.iter().all(|s| matches!(s, TextContent::Inline(_))));
    }
}

#[test]
fn menu_buffers_pack_rows_preserve_absent_parent_and_hidden_provider() {
    crate::i18n::init_for_tests();
    let (skins, _) = fixture(2, 3);
    let (mut a, mut b) = (rows(vec![]), rows(vec![]));
    let (mut ma, mut mb) = (menu(&skins), menu(&skins));
    a.display_order.clear();
    b.display_order.clear();
    ma.original_add_rows(&mut a, &[]);
    mb.add_rows(&mut b, &[]);
    assert_rows(&a, &b, &ma, &mb);
    a.display_order.push(RowId::NoteSkin);
    b.display_order.push(RowId::NoteSkin);
    ma.original_add_rows(&mut a, &[]);
    mb.add_rows(&mut b, &[]);
    assert_rows(&a, &b, &ma, &mb);
    assert_eq!(mb.choices[0], vec![None]);
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_menu_buffers_packs() {
    crate::i18n::init_for_tests();
    for (label, count, variants, enabled) in [
        ("bundled", 0, 0, true),
        ("two-providers", 2, 16, true),
        ("large", 8, 64, true),
        ("hidden", 8, 64, false),
    ] {
        let (skins, mut names) = fixture(count, variants);
        if !enabled {
            names.clear();
        }
        let (mut a, mut b) = (rows(names.clone()), rows(names));
        let (mut ma, mut mb) = (menu(&skins), menu(&skins));
        let players = [PlayerOptionsData::default(), PlayerOptionsData::default()];
        compare(
            &format!("packs/{label}"),
            || {
                ma.original_add_rows(black_box(&mut a), black_box(&players));
                black_box(&ma.choices);
                black_box(&a.rows);
            },
            || {
                mb.add_rows(black_box(&mut b), black_box(&players));
                black_box(&mb.choices);
                black_box(&b.rows);
            },
        );
    }
}
