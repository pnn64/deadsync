use std::fs;
use std::path::{Path, PathBuf};

use deadsync_theme_stepmania_395::{ResourceCatalog, ResourceCategory, ResourceLookupError};

fn resource(root: &Path, category: ResourceCategory, name: &str, data: &str) -> PathBuf {
    let directory = root.join(category.directory());
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(name);
    fs::write(&path, data).unwrap();
    path.canonicalize().unwrap()
}

#[test]
fn inherited_alias_resolves_to_a_local_override() {
    let base = tempfile::tempdir().unwrap();
    let active = tempfile::tempdir().unwrap();
    let category = ResourceCategory::Sounds;
    resource(
        base.path(),
        category,
        "Navigation change.redir",
        "\u{feff}Cue.WAV\r\n",
    );
    resource(base.path(), category, "Cue.wav", "base");
    let override_path = resource(active.path(), category, "cue.wav", "override");
    let resources = ResourceCatalog::open([active.path(), base.path()]).unwrap();

    assert_eq!(
        resources.resolve(category, "navigation CHANGE").unwrap(),
        Some(override_path.as_path())
    );
}

#[test]
fn logical_graphic_names_accept_texture_suffixes_and_explicit_names_are_exact() {
    let theme = tempfile::tempdir().unwrap();
    let category = ResourceCategory::Graphics;
    let graphic = resource(theme.path(), category, "Panel art 4x1.png", "preview");
    resource(
        theme.path(),
        category,
        "Panel art.txt.db",
        "unrelated metadata",
    );
    let resources = ResourceCatalog::open([theme.path()]).unwrap();

    assert_eq!(
        resources.resolve(category, "Panel art").unwrap(),
        Some(graphic.as_path())
    );
    assert_eq!(resources.resolve(category, "Panel art.png").unwrap(), None);
    assert_eq!(
        resources.resolve(category, "Panel art 4X1.PNG").unwrap(),
        Some(graphic.as_path())
    );
}

#[test]
fn resolves_background_directories_and_legacy_number_fonts() {
    let theme = tempfile::tempdir().unwrap();
    let category = ResourceCategory::Backgrounds;
    let background = theme.path().join(category.directory()).join("Loop");
    fs::create_dir_all(&background).unwrap();
    resource(theme.path(), category, "Menu idle.redir", "Loop");
    let digits = resource(
        theme.path(),
        ResourceCategory::Numbers,
        "Digits.png",
        "glyphs",
    );
    resource(
        theme.path(),
        ResourceCategory::Fonts,
        "Result digits.redir",
        "Digits",
    );
    let resources = ResourceCatalog::open([theme.path()]).unwrap();

    assert_eq!(
        resources.resolve(category, "Menu idle").unwrap(),
        Some(background.canonicalize().unwrap().as_path())
    );
    assert_eq!(
        resources
            .resolve(ResourceCategory::Fonts, "Result digits")
            .unwrap(),
        Some(digits.as_path())
    );
}

#[test]
fn reports_ambiguous_assets_and_broken_redirects() {
    let theme = tempfile::tempdir().unwrap();
    let category = ResourceCategory::Graphics;
    resource(theme.path(), category, "Panel new.png", "one");
    resource(theme.path(), category, "Panel old.png", "two");
    resource(theme.path(), category, "Alias first.redir", "Alias second");
    resource(theme.path(), category, "Alias second.redir", "Alias first");
    resource(
        theme.path(),
        category,
        "Missing.redir",
        "No matching graphic",
    );
    resource(theme.path(), category, "Invalid.redir", "../Other/package");
    let resources = ResourceCatalog::open([theme.path()]).unwrap();

    assert!(matches!(
        resources.resolve(category, "Panel"),
        Err(ResourceLookupError::Ambiguous { .. })
    ));
    assert!(matches!(
        resources.resolve(category, "Alias first"),
        Err(ResourceLookupError::CircularRedirect(_))
    ));
    assert!(matches!(
        resources.resolve(category, "Missing"),
        Err(ResourceLookupError::MissingRedirectTarget { .. })
    ));
    assert!(matches!(
        resources.resolve(category, "Invalid"),
        Err(ResourceLookupError::InvalidRedirect { .. })
    ));
}
