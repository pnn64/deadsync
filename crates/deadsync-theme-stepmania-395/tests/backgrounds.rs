use std::fs;
use std::io;
use std::path::Path;

use deadlib_present::actors::{Actor, SizeSpec, SpriteSource};
use deadsync_theme_stepmania_395::{BackgroundTexture, ThemePackage};

fn write(root: &Path, path: &str, value: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}

#[test]
fn ordered_background_layers_make_drawable_sprites_with_local_timing() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "metrics.ini",
        "[ScreenPanel]\nFallback=ScreenShared\n",
    );
    write(root.path(), "Graphics/cover.png", "fixture");
    write(
        root.path(),
        "BGAnimations/ScreenShared backdrop/marker.png",
        "fixture",
    );
    write(
        root.path(),
        "BGAnimations/ScreenShared backdrop/BGAnimation.ini",
        concat!(
            "[BGAnimation]\nLengthSeconds=1.25\n",
            "[Layer10]\nFile=marker\nType=sprite\nTexCoordVelocityX=0.5\n",
            "OnCommand=stretchto,0,0,80,60;texturewrapping,0;linear,0.5;addx,50\n",
            "[Layer2]\nFile=..\\..\\Graphics\\cover.png\nType=1 // stretch\n",
            "Command=linear,1;diffusealpha,0\n",
        ),
    );
    let package = ThemePackage::open([root.path()], None).unwrap();
    let background = package
        .background("ScreenPanel", "backdrop", [640.0, 480.0])
        .unwrap()
        .unwrap();
    let mut bound = Vec::new();
    let mut playback = background
        .start(|path, repeat| {
            let name = path.file_stem().unwrap().to_str().unwrap();
            bound.push((name.to_owned(), repeat));
            Some(BackgroundTexture {
                source: SpriteSource::TextureStatic(if name == "cover" {
                    "cover"
                } else {
                    "marker"
                }),
                dimensions: [32.0, 16.0],
            })
        })
        .unwrap();
    assert_eq!(
        bound,
        [("cover".to_owned(), false), ("marker".to_owned(), true)]
    );

    let mut actors = Vec::new();
    playback.advance(0.25);
    playback.push_actors(&mut actors, -5);
    assert!(matches!(
        actors.as_slice(),
        [
            Actor::Sprite {
                source: SpriteSource::TextureStatic("cover"),
                size: [SizeSpec::Px(640.0), SizeSpec::Px(480.0)],
                offset: [320.0, 240.0],
                tint: [1.0, 1.0, 1.0, 0.75],
                z: -5,
                ..
            },
            Actor::Sprite {
                source: SpriteSource::TextureStatic("marker"),
                size: [SizeSpec::Px(80.0), SizeSpec::Px(60.0)],
                offset: [65.0, 30.0],
                uv_rect: Some([0.125, 0.0, 1.125, 1.0]),
                z: -4,
                ..
            }
        ]
    ));
    playback.advance(0.75);
    assert!(!playback.is_finished());
    playback.advance(0.25);
    assert!(playback.is_finished());
}

#[test]
fn malformed_or_unsupported_layers_are_reported_before_binding_textures() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "BGAnimations/ScreenPanel foreign/BGAnimation.ini",
        "[Layer1]\nFile=../../../outside.png\n",
    );
    write(
        root.path(),
        "BGAnimations/ScreenPanel grid/BGAnimation.ini",
        "[Layer1]\nType=tiles\nFile=marker.png\n",
    );
    write(
        root.path(),
        "BGAnimations/ScreenPanel ambiguous/BGAnimation.ini",
        "[Layer1]\nFile=marker\n",
    );
    for name in ["marker left.png", "marker right.png"] {
        let file = format!("BGAnimations/ScreenPanel ambiguous/{name}");
        write(root.path(), &file, "fixture");
    }
    let package = ThemePackage::open([root.path()], None).unwrap();
    assert!(
        package
            .background("ScreenPanel", "empty", [640.0, 480.0])
            .unwrap()
            .is_none()
    );

    let foreign = package
        .background("ScreenPanel", "foreign", [640.0, 480.0])
        .unwrap_err();
    assert_eq!(foreign.kind(), io::ErrorKind::InvalidData);
    assert!(foreign.to_string().contains("[Layer1] File"));
    let grid = package
        .background("ScreenPanel", "grid", [640.0, 480.0])
        .unwrap_err();
    assert_eq!(grid.kind(), io::ErrorKind::Unsupported);
    assert!(grid.to_string().contains("[Layer1] layer type"));

    let ambiguous = package
        .background("ScreenPanel", "ambiguous", [640.0, 480.0])
        .unwrap_err();
    assert_eq!(ambiguous.kind(), io::ErrorKind::InvalidData);
    assert!(ambiguous.to_string().contains("ambiguous sprite name"));
}
