use std::fs;
use std::path::{Path, PathBuf};

use deadsync_theme_stepmania_395::{ResourceCategory, ResourceLookupError, ThemePackage};

fn write(root: &Path, name: &str, data: &str) -> PathBuf {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, data).unwrap();
    path.canonicalize().unwrap()
}

#[test]
fn metrics_preserve_package_and_language_precedence() {
    let theme = tempfile::tempdir().unwrap();
    let base = tempfile::tempdir().unwrap();
    write(
        theme.path(),
        "metrics.ini",
        "[ScreenPanel]\nFallback=ScreenParent\nEmpty=\n[ScreenParent]\nOffset=@10 + 2\n",
    );
    write(
        base.path(),
        "metrics.ini",
        "[ScreenPanel]\nOffset=30\nHeight=100\nLabel=Base\nEmpty=base\n",
    );
    write(
        theme.path(),
        "Languages/english.ini",
        "\u{feff}[ScreenPanel]\nLabel=Menu\nHeader=Welcome\n",
    );
    write(
        theme.path(),
        "Languages/spanish.ini",
        "RootValue=hola\n[ScreenPanel]\nLabel=Menú\n",
    );
    let paths = [theme.path(), base.path()];
    let package = ThemePackage::open(paths, Some("spanish")).unwrap();

    assert_eq!(
        package.metric("ScreenPanel", "Label").unwrap(),
        Some("Menú")
    );
    assert_eq!(
        package.metric("ScreenPanel", "Header").unwrap(),
        Some("Welcome")
    );
    assert_eq!(
        package.metric("ScreenPanel", "Offset").unwrap(),
        Some("@10 + 2")
    );
    assert_eq!(
        package.metric("ScreenPanel", "Height").unwrap(),
        Some("100")
    );
    assert_eq!(package.metric("ScreenPanel", "Empty").unwrap(), Some(""));
    assert_eq!(package.metric("ScreenPanel", "Unknown").unwrap(), None);
    assert_eq!(package.metric("", "RootValue").unwrap(), Some("hola"));

    let english = ThemePackage::open(paths, None).unwrap();
    assert_eq!(
        english.metric("ScreenPanel", "Label").unwrap(),
        Some("Menu")
    );
}

#[test]
fn screen_resources_inherit_within_each_package_and_follow_scoped_redirects() {
    let theme = tempfile::tempdir().unwrap();
    let base = tempfile::tempdir().unwrap();
    write(
        base.path(),
        "metrics.ini",
        "[ScreenPanel]\nFallback=ScreenParent\n[ScreenSource]\nFallback=ScreenCommon\n",
    );
    write(base.path(), "Graphics/ScreenPanel heading.png", "base");
    write(
        theme.path(),
        "Graphics/ScreenParent heading.redir",
        "ScreenSource title",
    );
    write(base.path(), "Graphics/ScreenSource title.png", "base");
    let graphic = write(theme.path(), "Graphics/ScreenCommon title.png", "active");
    let catalog = ThemePackage::open([theme.path(), base.path()], None).unwrap();

    assert_eq!(
        catalog
            .screen_resource(ResourceCategory::Graphics, "ScreenPanel", "heading")
            .unwrap(),
        Some(graphic.as_path())
    );
}

#[test]
fn circular_fallbacks_distinguish_metrics_and_resources() {
    let theme = tempfile::tempdir().unwrap();
    write(
        theme.path(),
        "metrics.ini",
        "[ScreenA]\nFallback=ScreenB\n[ScreenB]\nFallback=ScreenA\n",
    );
    let catalog = ThemePackage::open([theme.path()], None).unwrap();

    let metric_error = catalog.metric("ScreenA", "Header").unwrap_err();
    assert_eq!(metric_error.section, "ScreenA");
    assert_eq!(metric_error.key, "Header");
    assert!(matches!(
        catalog.screen_resource(ResourceCategory::Graphics, "ScreenA", "heading"),
        Err(ResourceLookupError::CircularSectionFallback(section)) if section == "ScreenA"
    ));
}
