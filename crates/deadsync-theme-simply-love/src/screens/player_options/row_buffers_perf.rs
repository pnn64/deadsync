use super::*;


fn labels(count: usize, long: bool) -> Vec<String> {
    (0..count)
        .map(|i| {
            if long {
                format!("Palette Å猫 {i} with a localized name beyond inline storage")
            } else {
                format!("Palette {i}")
            }
        })
        .collect()
}


#[test]
fn palette_replacement_invalidates_layout_and_preserves_each_player_selection() {
    use super::super::apply_judgment_palette_choices;
    let mut map = RowMap::new();
    // A missing palette row still does nothing.
    apply_judgment_palette_choices(&mut map, &[], &[], &[None, None]);
    let mut row = Row::exit();
    row.id = RowId::JudgmentColors;
    row.choice_widths = vec![5.0, 8.0].into_boxed_slice();
    row.choice_offsets = vec![0.0, 9.0].into_boxed_slice();
    row.choice_height = 20.0;
    map.insert(row);
    let choices = labels(3, false);
    let ids = vec![None, Some("chosen".to_owned()), Some("chosen".to_owned())];
    apply_judgment_palette_choices(
        &mut map,
        &choices,
        &ids,
        &[Some("chosen".into()), Some("missing".into())],
    );
    let row = map.row(RowId::JudgmentColors);
    assert_eq!(row.selected_choice_index, [1, 0]);
    assert!(row.choice_widths.is_empty() && row.choice_offsets.is_empty());
    assert_eq!(row.choice_height, 0.0);
    assert_eq!(
        row.choices
            .iter()
            .map(TextContent::as_str)
            .collect::<Vec<_>>(),
        choices.iter().map(String::as_str).collect::<Vec<_>>()
    );
    apply_judgment_palette_choices(&mut map, &[], &[], &[None, None]);
    assert!(map.row(RowId::JudgmentColors).choices.is_empty());
    assert_eq!(map.row(RowId::JudgmentColors).selected_choice_index, [0, 0]);
}
