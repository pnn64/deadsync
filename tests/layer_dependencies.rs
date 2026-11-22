//! Check production layering and theme-free game-controller test builds, including
//! dependencies hidden by aliases, optional features, and target/build tables.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::process::Command;

#[derive(serde::Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(serde::Deserialize)]
struct Package {
    id: String,
    name: String,
    dependencies: Vec<Dependency>,
}

#[derive(serde::Deserialize)]
struct Dependency {
    // Cargo reports the package name here, even when the manifest renames it.
    name: String,
    kind: Option<String>,
}

#[derive(Clone, Copy)]
enum Layer {
    Engine,
    Game,
    Theme,
    Application,
    Other,
}

fn layer(name: &str) -> Layer {
    match name {
        // These two composition roots wire game services and the selected theme.
        "deadsync" | "deadsync-shell" => Layer::Application,
        "deadsync-theme" => Layer::Theme,
        _ if name.starts_with("deadsync-theme-") => Layer::Theme,
        _ if name.starts_with("deadlib-") => Layer::Engine,
        _ if name.starts_with("deadsync-") => Layer::Game,
        _ => Layer::Other,
    }
}

fn points_upward(source: Layer, dependency: Layer) -> bool {
    match source {
        Layer::Engine => matches!(dependency, Layer::Game | Layer::Theme | Layer::Application),
        Layer::Game => matches!(dependency, Layer::Theme | Layer::Application),
        Layer::Theme => matches!(dependency, Layer::Application),
        Layer::Application | Layer::Other => false,
    }
}

fn violations(metadata: &Metadata) -> Vec<String> {
    dependency_violations(metadata, None)
}

fn dependency_violations(metadata: &Metadata, test_owner: Option<&str>) -> Vec<String> {
    let members: BTreeSet<_> = metadata
        .workspace_members
        .iter()
        .map(String::as_str)
        .collect();
    let packages: Vec<_> = metadata
        .packages
        .iter()
        .filter(|package| members.contains(package.id.as_str()))
        .collect();
    assert!(!members.is_empty(), "Cargo reported no workspace members");
    assert_eq!(packages.len(), members.len(), "missing workspace packages");

    let graph: BTreeMap<_, Vec<_>> = packages
        .iter()
        .map(|package| {
            let mut dependencies: Vec<_> = package
                .dependencies
                .iter()
                // A test build includes the owner's dev dependencies, but not
                // those belonging to its dependencies. The general production
                // graph excludes all dev edges. Both include optional edges
                // and target declarations beyond the platform running this test.
                .filter(|dependency| {
                    dependency.kind.as_deref() != Some("dev")
                        || test_owner == Some(package.name.as_str())
                })
                .map(|dependency| dependency.name.as_str())
                .collect();
            dependencies.sort_unstable();
            dependencies.dedup();
            (package.name.as_str(), dependencies)
        })
        .collect();
    assert_eq!(
        graph.len(),
        packages.len(),
        "ambiguous workspace package names"
    );
    if let Some(owner) = test_owner {
        assert!(
            graph.contains_key(owner),
            "missing test-build owner {owner}"
        );
    }

    let mut failures = Vec::new();
    for &source in graph.keys() {
        if test_owner.is_some_and(|owner| source != owner) {
            continue;
        }
        let source_layer = layer(source);
        if matches!(source_layer, Layer::Application | Layer::Other) {
            continue;
        }
        let mut visited = BTreeSet::from([source]);
        let mut pending = VecDeque::from([vec![source]]);
        while let Some(path) = pending.pop_front() {
            let Some(dependencies) = graph.get(path.last().unwrap()) else {
                // --no-deps does not resolve third-party dependency internals.
                continue;
            };
            for &dependency in dependencies {
                if !visited.insert(dependency) {
                    continue;
                }
                let mut next = path.clone();
                next.push(dependency);
                if points_upward(source_layer, layer(dependency)) {
                    failures.push(next.join(" -> "));
                } else {
                    pending.push_back(next);
                }
            }
        }
    }
    failures
}

fn workspace_metadata() -> Metadata {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .current_dir(root)
        .args([
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("run cargo metadata for the workspace dependency graph");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("Cargo metadata JSON")
}

#[test]
fn production_crate_dependencies_follow_layer_direction() {
    let failures = violations(&workspace_metadata());
    assert!(
        failures.is_empty(),
        "production dependencies must point from themes to game to engine; \
         only deadsync and deadsync-shell may compose all layers:\n{}",
        failures.join("\n")
    );
}

#[test]
fn game_controller_test_builds_stay_independent_of_themes_and_application() {
    let metadata = workspace_metadata();
    for owner in [
        "deadsync-assets",
        "deadsync-gameplay",
        "deadsync-online",
        "deadsync-song-lua",
    ] {
        let failures = dependency_violations(&metadata, Some(owner));
        assert!(
            failures.is_empty(),
            "{owner} behavior must be testable without a theme or application; \
             its test build has upward dependencies:\n{}",
            failures.join("\n")
        );
    }
}

#[test]
fn physical_input_contract_does_not_own_game_device_slots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("crates/deadlib-platform/src/input.rs"))
        .expect("physical input contract");
    let identifiers: BTreeSet<_> = source
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .collect();
    for game_policy in ["REFLEX_PAD_ID_BASE", "PAD_ID_COUNT_CAP"] {
        assert!(
            !identifiers.contains(game_policy),
            "deadlib-platform owns game device allocation policy: {game_policy}"
        );
    }
}

fn fixture(packages: serde_json::Value) -> Metadata {
    let mut packages = packages.as_array().expect("fixture packages").clone();
    let mut members = Vec::new();
    for package in &mut packages {
        let id = package["name"].clone();
        package["id"] = id.clone();
        members.push(id);
    }
    serde_json::from_value(serde_json::json!({
        "packages": packages,
        "workspace_members": members,
    }))
    .expect("fixture metadata")
}

#[test]
fn guard_checks_aliases_optional_targets_and_build_dependencies() {
    let metadata = fixture(serde_json::json!([
        { "name": "deadlib-future", "dependencies": [
            { "name": "deadsync-future", "rename": "generic_alias", "kind": "build",
              "target": "cfg(target_os = \"other\")", "optional": true }
        ] },
        { "name": "deadsync-future", "dependencies": [
            { "name": "deadsync-theme-future", "rename": "style", "kind": null }
        ] },
        { "name": "deadsync-theme-future", "dependencies": [
            { "name": "deadsync-shell", "kind": null }
        ] },
        { "name": "deadsync-shell", "dependencies": [] }
    ]));
    assert_eq!(
        violations(&metadata),
        [
            "deadlib-future -> deadsync-future",
            "deadsync-future -> deadsync-theme-future",
            "deadsync-theme-future -> deadsync-shell",
        ]
    );
}

#[test]
fn guard_follows_workspace_bridges_and_ignores_dev_edges() {
    let metadata = fixture(serde_json::json!([
        { "name": "deadlib-future", "dependencies": [
            { "name": "helper", "kind": null },
            { "name": "deadsync-theme-future", "kind": "dev" }
        ] },
        { "name": "helper", "dependencies": [
            { "name": "deadsync-future", "kind": null }
        ] },
        { "name": "deadsync-future", "dependencies": [
            { "name": "deadsync-theme-future", "kind": "dev" }
        ] },
        { "name": "deadsync-theme-future", "dependencies": [] }
    ]));
    assert_eq!(
        violations(&metadata),
        ["deadlib-future -> helper -> deadsync-future"]
    );
}

#[test]
fn guard_accepts_downward_dependencies_and_application_composition() {
    let metadata = fixture(serde_json::json!([
        { "name": "deadsync", "dependencies": [
            { "name": "deadsync-shell", "kind": null }
        ] },
        { "name": "deadsync-shell", "dependencies": [
            { "name": "deadsync-theme-future", "kind": null }
        ] },
        { "name": "deadsync-theme-future", "dependencies": [
            { "name": "deadsync-future", "kind": null }
        ] },
        { "name": "deadsync-future", "dependencies": [
            { "name": "deadlib-future", "kind": null }
        ] },
        { "name": "deadlib-future", "dependencies": [
            { "name": "deadsync-future", "kind": "dev" }
        ] }
    ]));
    assert!(violations(&metadata).is_empty());
}

#[test]
fn test_build_guard_checks_owner_dependencies_and_transitive_production_edges() {
    for owner_kind in [None, Some("build"), Some("dev")] {
        for transitive_kind in [None, Some("build")] {
            for forbidden in [
                "deadsync-theme",
                "deadsync-theme-future",
                "deadsync-shell",
                "deadsync",
            ] {
                let metadata = fixture(serde_json::json!([
                    { "name": "deadsync-song-lua", "dependencies": [
                        { "name": "test-helper", "rename": "fixture", "kind": owner_kind,
                          "target": "cfg(target_os = \"other\")", "optional": true }
                    ] },
                    { "name": "test-helper", "dependencies": [
                        { "name": forbidden, "rename": "adapter", "kind": transitive_kind }
                    ] },
                    { "name": forbidden, "dependencies": [] }
                ]));
                assert_eq!(
                    dependency_violations(&metadata, Some("deadsync-song-lua")),
                    [format!("deadsync-song-lua -> test-helper -> {forbidden}")],
                    "owner edge {owner_kind:?}, transitive edge {transitive_kind:?}"
                );
                if owner_kind == Some("dev") {
                    assert!(violations(&metadata).is_empty());
                }
            }
        }
    }
}

#[test]
fn test_build_guard_excludes_dependency_dev_edges_and_unrelated_packages() {
    let metadata = fixture(serde_json::json!([
        { "name": "deadsync-song-lua", "dependencies": [
            { "name": "test-helper", "kind": "dev" }
        ] },
        { "name": "test-helper", "dependencies": [
            { "name": "deadlib-future", "kind": null },
            { "name": "deadsync-theme-future", "kind": "dev" }
        ] },
        { "name": "deadlib-future", "dependencies": [
            { "name": "deadsync-shell", "kind": "dev" }
        ] },
        { "name": "deadsync-unrelated", "dependencies": [
            { "name": "deadsync-theme-future", "kind": null }
        ] },
        { "name": "deadsync-theme-future", "dependencies": [] },
        { "name": "deadsync-shell", "dependencies": [] }
    ]));
    assert!(dependency_violations(&metadata, Some("deadsync-song-lua")).is_empty());
    assert_eq!(
        violations(&metadata),
        ["deadsync-unrelated -> deadsync-theme-future"],
        "unrelated production violations still belong to the general guard"
    );
}
