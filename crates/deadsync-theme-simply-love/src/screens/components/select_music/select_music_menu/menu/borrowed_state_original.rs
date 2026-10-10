use super::*;

pub fn build_entries(lists: &CategoryItemLists, categories: &CategoryState) -> Vec<Entry> {
    build_entries_from_slices(
        &lists.standalone,
        &lists.sorts,
        lists.profile.as_deref(),
        &lists.advanced,
        lists.pad_profile.as_deref(),
        lists.styles.as_deref(),
        lists.playlists.as_deref(),
        categories,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_entries_from_slices(
    items_standalone: &[Item],
    items_sorts: &[Item],
    items_profile: Option<&[Item]>,
    items_advanced: &[Item],
    items_pad_profile: Option<&[Item]>,
    items_styles: Option<&[Item]>,
    items_playlists: Option<&[Item]>,
    categories: &CategoryState,
) -> Vec<Entry> {
    // If a category is expanded, show ONLY that category header + its items
    // (wrapping/repeating in the wheel). This matches Simply Love's behavior.
    if categories.is_expanded(Category::Sorts) {
        let mut entries = vec![Entry::CategoryHeader {
            category: Category::Sorts,
            label: "Sorts...",
        }];
        for item in items_sorts {
            entries.push(Entry::CategoryItem(item.clone()));
        }
        return entries;
    }
    if categories.is_expanded(Category::Profile)
        && let Some(profile_items) = items_profile
    {
        let mut entries = vec![Entry::CategoryHeader {
            category: Category::Profile,
            label: "Profile...",
        }];
        for item in profile_items {
            entries.push(Entry::CategoryItem(item.clone()));
        }
        return entries;
    }
    if categories.is_expanded(Category::Advanced) {
        let mut entries = vec![Entry::CategoryHeader {
            category: Category::Advanced,
            label: "Advanced...",
        }];
        for item in items_advanced {
            entries.push(Entry::CategoryItem(item.clone()));
        }
        return entries;
    }
    if categories.is_expanded(Category::PadProfile)
        && let Some(pad_profile_items) = items_pad_profile
    {
        let mut entries = vec![Entry::CategoryHeader {
            category: Category::PadProfile,
            label: "Pad Profile...",
        }];
        for item in pad_profile_items {
            entries.push(Entry::CategoryItem(item.clone()));
        }
        return entries;
    }
    if categories.is_expanded(Category::Styles)
        && let Some(style_items) = items_styles
    {
        let mut entries = vec![Entry::CategoryHeader {
            category: Category::Styles,
            label: "Styles...",
        }];
        for item in style_items {
            entries.push(Entry::CategoryItem(item.clone()));
        }
        return entries;
    }
    if categories.is_expanded(Category::Playlists)
        && let Some(playlist_items) = items_playlists
    {
        let mut entries = vec![Entry::CategoryHeader {
            category: Category::Playlists,
            label: "Playlists...",
        }];
        for item in playlist_items {
            entries.push(Entry::CategoryItem(item.clone()));
        }
        return entries;
    }

    // No category expanded — show all standalone items + collapsed category headers
    let mut entries = Vec::new();

    for item in items_standalone {
        entries.push(Entry::StandaloneItem(item.clone()));
    }

    entries.push(Entry::CategoryHeader {
        category: Category::Sorts,
        label: "Sorts...",
    });

    if items_profile.is_some() {
        entries.push(Entry::CategoryHeader {
            category: Category::Profile,
            label: "Profile...",
        });
    }

    entries.push(Entry::CategoryHeader {
        category: Category::Advanced,
        label: "Advanced Options",
    });

    if items_pad_profile.is_some() {
        entries.push(Entry::CategoryHeader {
            category: Category::PadProfile,
            label: "Pad Profile...",
        });
    }

    if items_styles.is_some() {
        entries.push(Entry::CategoryHeader {
            category: Category::Styles,
            label: "Styles...",
        });
    }
    if items_playlists.is_some() {
        entries.push(Entry::CategoryHeader {
            category: Category::Playlists,
            label: "Playlists...",
        });
    }

    entries
}
