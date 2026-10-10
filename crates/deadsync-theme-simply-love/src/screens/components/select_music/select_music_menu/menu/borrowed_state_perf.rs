use super::*;
use deadlib_present::actors::TextContent;
use std::hint::black_box;
use std::sync::Arc;

const CATEGORIES: [Category; 6] = [
    Category::Sorts,
    Category::Profile,
    Category::Advanced,
    Category::PadProfile,
    Category::Styles,
    Category::Playlists,
];

fn categories(mask: usize) -> CategoryState {
    CategoryState {
        expanded: CATEGORIES
            .into_iter()
            .enumerate()
            .filter_map(|(i, category)| (mask & (1 << i) != 0).then_some(category))
            .collect(),
    }
}

fn items(count: usize, owned: bool) -> Vec<Item> {
    (0..count)
        .map(|i| {
            if owned {
                Item {
                    top_label: TextContent::Owned(format!("Playlist {i}: 日本語")),
                    bottom_label: TextContent::Shared(Arc::from(format!("お気に入り {i}"))),
                    action: Action::SortByPlaylist(format!("playlist-identifier-{i:08}")),
                }
            } else {
                Item {
                    top_label: TextContent::Static("Sort By"),
                    bottom_label: TextContent::Static("Title"),
                    action: Action::SortByTitle,
                }
            }
        })
        .collect()
}

fn lists(optional: usize, count: usize, owned: bool) -> CategoryItemLists {
    CategoryItemLists {
        standalone: items(3, owned),
        sorts: items(count, owned),
        profile: (optional & 1 != 0).then(|| items(count, owned)),
        advanced: items(count, owned),
        pad_profile: (optional & 2 != 0).then(|| items(count, owned)),
        styles: (optional & 4 != 0).then(|| items(count, owned)),
        playlists: (optional & 8 != 0).then(|| items(count, owned)),
    }
}

fn copy_lists(source: &CategoryItemLists) -> CategoryItemLists {
    CategoryItemLists {
        standalone: source.standalone.clone(),
        sorts: source.sorts.clone(),
        profile: source.profile.clone(),
        advanced: source.advanced.clone(),
        pad_profile: source.pad_profile.clone(),
        styles: source.styles.clone(),
        playlists: source.playlists.clone(),
    }
}

#[test]
fn owned_menu_entries_preserve_every_category_combination() {
    for optional in 0..16 {
        for count in [0, 3] {
            let source = lists(optional, count, true);
            for mask in 0..64 {
                let categories = categories(mask);
                let before = borrowed_state_original::build_entries(&source, &categories);
                let after = build_entries(copy_lists(&source), &categories);
                assert_eq!(
                    format!("{before:?}"),
                    format!("{after:?}"),
                    "optional={optional}, count={count}, expanded={mask}"
                );
            }
        }
    }
}

#[test]
fn owned_menu_entries_move_strings_and_allocate_only_the_result() {
    let source = lists(8, 64, true);
    let moved = copy_lists(&source);
    let pointer = moved.playlists.as_ref().unwrap()[0]
        .top_label
        .as_str()
        .as_ptr();
    let categories = categories(1 << 5);
    let (before, old) =
        crate::perf::measure(|| borrowed_state_original::build_entries(&source, &categories));
    let (after, new) = crate::perf::measure(|| build_entries(moved, &categories));
    let Entry::CategoryItem(first) = &after[1] else {
        panic!("expected first playlist");
    };
    assert_eq!(first.top_label.as_str().as_ptr(), pointer);
    assert_eq!(format!("{before:?}"), format!("{after:?}"));
    assert_eq!(new.allocs, 1);
    assert_eq!(new.reallocs, 0);
    assert!(old.allocs >= 129);
    assert!(new.allocated_bytes < old.allocated_bytes);
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_ui_borrowed_state_menu() {
    for (name, optional, count, mask) in [
        ("collapsed-static", 0, 0, 0),
        ("sorts-static", 0, 0, 1),
        ("profile-static", 1, 8, 2),
        ("pad-32", 2, 32, 8),
        ("playlists-100", 8, 100, 32),
        ("playlists-1024", 8, 1024, 32),
        ("empty-expanded", 8, 0, 32),
    ] {
        let mut source = lists(optional, 0, false);
        source.standalone = items(5, false);
        source.sorts = super::super::SORT_ITEMS.to_vec();
        source.advanced = items(10, false);
        if optional == 1 {
            source.profile = Some(items(count, false));
        } else if optional == 2 {
            source.pad_profile = Some(
                (0..count)
                    .map(|i| {
                        super::super::pad_profile_item(
                            "P1 Pad Profile",
                            format!("Saved {i}"),
                            false,
                            false,
                            format!("profile-{i}"),
                            i == 0,
                        )
                    })
                    .collect(),
            );
        } else if optional == 8 {
            source.playlists = Some(
                (0..count)
                    .map(|i| {
                        super::super::playlist_item(
                            "Machine Playlist",
                            format!("お気に入り {i}"),
                            format!("playlist-identifier-{i:08}"),
                        )
                    })
                    .collect(),
            );
        }
        let categories = categories(mask);
        // Both pipelines receive freshly owned lists, as production callers do.
        // Include identical input materialization and all destruction in timing.
        crate::ui_borrowed_state_support::compare(
            &format!("menu/{name}"),
            || {
                let input = copy_lists(black_box(&source));
                black_box(borrowed_state_original::build_entries(
                    &input,
                    black_box(&categories),
                ));
            },
            || {
                let input = copy_lists(black_box(&source));
                black_box(build_entries(input, black_box(&categories)));
            },
        );
    }
}
