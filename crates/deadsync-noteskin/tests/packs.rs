use deadsync_noteskin::model::itg_resolve_model_texture_path;
use deadsync_noteskin::pack::{InstalledPack, Selection, checked_path};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("deadsync-pack-{}-{stamp}", std::process::id()));
        for dir in [
            "base",
            "choices/blue",
            "choices/small",
            "fallback/dance/common",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(
            root.join("base/metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n[NoteDisplay]\nTapNoteAnimationLength=2\n",
        )
        .unwrap();
        fs::write(root.join("base/NoteSkin.lua"), "return {}\n").unwrap();
        fs::write(
            root.join("fallback/dance/common/metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n[NoteDisplay]\nHoldBodyOffsetY=7\n",
        )
        .unwrap();
        for file in [
            "base/_mine tex.png",
            "base/_Down Tap Note.png",
            "choices/blue/_mine tex.png",
            "choices/blue/arrow.png",
        ] {
            image::RgbaImage::new(4, 4).save(root.join(file)).unwrap();
        }
        for prefix in ["base", "choices/small"] {
            fs::write(
                root.join(prefix).join("_mine model.txt"),
                "\"_mine ani tex.ini\"\n",
            )
            .unwrap();
            fs::write(
                root.join(prefix).join("_mine ani tex.ini"),
                "[AnimatedTexture]\nFrame0000=_mine tex.png\nDelay0000=1\nTexVelocityX=1\n",
            )
            .unwrap();
        }
        image::RgbaImage::new(2048, 2048)
            .save(root.join("preview.png"))
            .unwrap();
        let fixture = Self(root);
        fixture.write(&json!({
            "schema": 1, "id": "sample", "version": "1", "source": "fixture",
            "skins": [{"id": "sample-cel", "base": "base", "preview": "preview.png", "options": [
                {"slot": "arrows", "id": "blue", "label": "Blue", "cell": 0,
                 "files": [{"target": "_Down Tap Note.png", "source": "choices/blue/arrow.png"}],
                 "metrics": [{"section": "NoteDisplay", "key": "TapNoteAnimationLength", "value": "4"}]},
                {"slot": "mines", "id": "blue", "label": "Blue", "cell": 1,
                 "files": [{"target": "_mine tex.png", "source": "choices/blue/_mine tex.png"}]},
                {"slot": "mine_size", "id": "small", "label": "Small", "cell": 2,
                 "files": [
                     {"target": "_mine model.txt", "source": "choices/small/_mine model.txt"},
                     {"target": "_mine ani tex.ini", "source": "choices/small/_mine ani tex.ini"}
                 ]}
            ]}]
        }));
        fixture
    }

    fn write(&self, value: &Value) {
        fs::write(self.0.join("pack.json"), serde_json::to_vec(value).unwrap()).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let target = self.0.canonicalize().unwrap();
        let parent = std::env::temp_dir().canonicalize().unwrap();
        assert!(target.parent() == Some(parent.as_path()));
        fs::remove_dir_all(target).unwrap();
    }
}

#[test]
fn player_selections_resolve_independently_including_relocated_model_textures() {
    let fixture = Fixture::new();
    let pack = InstalledPack::load(&fixture.0).unwrap();
    let roots = [fixture.0.join("fallback")];
    let original = pack
        .resolve(&Selection::parse("sample-cel").unwrap(), &roots)
        .unwrap();
    let customized = pack
        .resolve(
            &Selection::parse("sample-cel?arrows=blue&mines=blue&mine_size=small").unwrap(),
            &roots,
        )
        .unwrap();
    assert_eq!(
        original.get_metric("NoteDisplay", "TapNoteAnimationLength"),
        Some("2")
    );
    assert_eq!(
        customized.get_metric("NoteDisplay", "TapNoteAnimationLength"),
        Some("4")
    );
    assert_eq!(
        customized.get_metric("NoteDisplay", "HoldBodyOffsetY"),
        Some("7")
    );
    assert_eq!(
        original.resolve_path("_Down", "Tap Note").unwrap(),
        fixture
            .0
            .join("base/_Down Tap Note.png")
            .canonicalize()
            .unwrap()
    );
    assert_eq!(
        customized.resolve_path("_Down", "Tap Note").unwrap(),
        fixture
            .0
            .join("choices/blue/arrow.png")
            .canonicalize()
            .unwrap()
    );
    for (data, expected) in [
        (&original, "base/_mine tex.png"),
        (&customized, "choices/blue/_mine tex.png"),
    ] {
        let model = data.resolve_path("", "_mine model").unwrap();
        let resolved = itg_resolve_model_texture_path(data, &model).unwrap();
        assert_eq!(
            resolved.texture_path,
            fixture.0.join(expected).canonicalize().unwrap()
        );
        assert_eq!(resolved.tex.uv_velocity, [1.0, 0.0]);
    }
    assert!(
        pack.resolve(
            &Selection::parse("sample-cel?arrows=missing").unwrap(),
            &roots
        )
        .is_err()
    );
}

#[test]
fn cache_identity_is_canonical_and_changes_with_selection_or_pack_revision() {
    let fixture = Fixture::new();
    let pack = InstalledPack::load(&fixture.0).unwrap();
    let a = Selection::parse("sample-cel?mines=blue&arrows=blue").unwrap();
    let b = Selection::parse("sample-cel?arrows=blue&mines=blue").unwrap();
    assert_eq!(a, b);
    assert_eq!(Selection::parse(&a.to_string()).unwrap(), a);
    assert_eq!(pack.runtime_key(&a), pack.runtime_key(&b));
    assert_ne!(
        pack.runtime_key(&a),
        pack.runtime_key(&Selection::parse("sample-cel").unwrap())
    );
    let mut manifest = serde_json::to_value(&pack.manifest).unwrap();
    manifest["version"] = "2".into();
    fixture.write(&manifest);
    let updated = InstalledPack::load(&fixture.0).unwrap();
    assert_ne!(pack.runtime_key(&a), updated.runtime_key(&a));
    assert_ne!(pack.compiler_key(&a), updated.compiler_key(&a));
}

#[test]
fn png_variants_share_programs_but_keep_runtime_assets_independent() {
    let fixture = Fixture::new();
    let pack = InstalledPack::load(&fixture.0).unwrap();
    let base = Selection::parse("sample-cel").unwrap();
    let png = Selection::parse("sample-cel?mines=blue").unwrap();
    assert_eq!(pack.compiler_key(&base), pack.compiler_key(&png));
    assert_ne!(pack.runtime_key(&base), pack.runtime_key(&png));
    let roots = [fixture.0.join("fallback")];
    assert_ne!(
        pack.resolve(&base, &roots)
            .unwrap()
            .resolve_path("", "_mine tex"),
        pack.resolve(&png, &roots)
            .unwrap()
            .resolve_path("", "_mine tex")
    );
    // Metric changes may be read by Lua; model/INI/script swaps stay isolated too.
    for name in ["sample-cel?arrows=blue", "sample-cel?mine_size=small"] {
        let selection = Selection::parse(name).unwrap();
        assert_ne!(pack.compiler_key(&base), pack.compiler_key(&selection));
        assert_eq!(pack.compiler_key(&selection), pack.runtime_key(&selection));
    }
    assert_eq!(
        pack.compiler_key(&Selection::parse("sample-cel?arrows=blue&mines=blue").unwrap()),
        pack.compiler_key(&Selection::parse("sample-cel?arrows=blue").unwrap())
    );
}

#[test]
fn invalid_manifests_and_escaping_paths_are_rejected() {
    let fixture = Fixture::new();
    let pack = InstalledPack::load(&fixture.0).unwrap();
    for path in [
        "../outside.png",
        "/absolute",
        "C:/absolute",
        "base\\_mine tex.png",
        "base/../preview.png",
    ] {
        assert!(checked_path(&fixture.0, path).is_err(), "{path}");
    }
    for raw in [
        "sample-cel?arrows=a&arrows=b",
        "sample-cel?unknown=x",
        "sample-cel?arrows=../x",
    ] {
        assert!(Selection::parse(raw).is_err(), "{raw}");
    }
    let mut manifest = serde_json::to_value(&pack.manifest).unwrap();
    manifest["skins"][0]["options"][0]["files"][0]["source"] = "../outside.png".into();
    fixture.write(&manifest);
    assert!(InstalledPack::load(&fixture.0).is_err());
    manifest["skins"][0]["options"][0]["files"][0]["source"] = "missing.png".into();
    fixture.write(&manifest);
    assert!(InstalledPack::load(&fixture.0).is_err());
}

#[test]
fn discovery_ignores_valid_install_backups() {
    let parent = Fixture::new();
    let backup = Fixture::new();
    let hidden = parent.0.join(".workshop-backup-test");
    fs::rename(&backup.0, &hidden).unwrap();
    assert!(InstalledPack::load(&hidden).is_ok());
    assert!(deadsync_noteskin::pack::discover(&[parent.0.clone()]).is_empty());
    let visible = parent.0.join("installed");
    fs::rename(&hidden, &visible).unwrap();
    let packs = deadsync_noteskin::pack::discover(&[parent.0.clone()]);
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].root, visible.canonicalize().unwrap());
    fs::rename(&visible, &backup.0).unwrap();
}
