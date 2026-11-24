use std::fs;
use std::path::Path;

use deadlib_assets::decode_texture;
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::space::{Metrics, with_current_metrics};
use deadsync_theme::attract::AttractPresentation;
use deadsync_theme_stepmania_395::PackageIntro;

#[test]
fn authored_intro_preloads_textures_and_replays_ordered_scenes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("metrics.ini"), "[ScreenCompany]\n[ScreenLogo]\n").unwrap();
    let graphics = root.join("Graphics");
    fs::create_dir(&graphics).unwrap();
    write_bitmap(&graphics.join("shape.bmp"));

    write_scene(
        root,
        "ScreenCompany",
        "[BGAnimation]\nLengthSeconds=1\n\
        [Layer1]\nType=1\nFile=../../Graphics/shape.bmp\n\
        [Layer2]\nFile=../../Graphics/shape.bmp\nCommand=x,12;y,24;\n",
    );
    write_scene(
        root,
        "ScreenLogo",
        "[Layer1]\nFile=../../Graphics/shape.bmp\n\
        TexCoordVelocityX=0.5\nCommand=x,40;y,60;sleep,0.5;\n",
    );

    let (mut intro, textures) = PackageIntro::open(root, None).unwrap();
    assert_eq!(textures.len(), 2);
    assert_ne!(textures[0].sampler, textures[1].sampler);
    for texture in &textures {
        assert_eq!(
            decode_texture(&texture.path, &texture.hints)
                .unwrap()
                .source_size,
            [1, 1]
        );
    }

    let first_frame = scene_actors(&intro);
    assert_eq!(first_frame.len(), 2);
    assert_eq!(
        sprite(&first_frame[0]),
        ([320.0, 240.0], textures[0].key.as_str())
    );
    assert_eq!(
        sprite(&first_frame[1]),
        ([12.0, 24.0], textures[0].key.as_str())
    );
    assert!(!intro.advance(0.75));
    assert!(!intro.advance(0.25));
    assert_eq!(intro.playback_time(), 0.0);
    let second_frame = scene_actors(&intro);
    assert_eq!(
        sprite(&second_frame[0]),
        ([40.0, 60.0], textures[1].key.as_str())
    );
    assert!(intro.advance(0.5));

    intro.reset();
    assert_eq!(intro.playback_time(), 0.0);
    assert_eq!(sprite(&scene_actors(&intro)[0]), sprite(&first_frame[0]));
}

fn scene_actors(intro: &PackageIntro) -> Vec<Actor> {
    let mut actors = Vec::new();
    with_current_metrics(Metrics::centered(854.0, 480.0), || {
        intro.push_actors(&mut actors, None);
    });
    let [
        Actor::Frame {
            children,
            size,
            offset,
            ..
        },
    ] = actors.as_mut_slice()
    else {
        panic!("expected a single centered intro canvas");
    };
    assert!(matches!(size, [SizeSpec::Px(640.0), SizeSpec::Px(480.0)]));
    assert_eq!(*offset, [427.0, 240.0]);
    std::mem::take(children)
}

fn sprite(actor: &Actor) -> ([f32; 2], &str) {
    let Actor::Sprite { offset, source, .. } = actor else {
        panic!("expected an authored sprite");
    };
    (*offset, source.texture_key().unwrap())
}

fn write_scene(root: &Path, screen: &str, ini: &str) {
    let directory = root
        .join("BGAnimations")
        .join(format!("{screen} background"));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("BGAnimation.ini"), ini).unwrap();
}

fn write_bitmap(path: &Path) {
    // 1 × 1 uncompressed 24-bit BMP, with one padded blue pixel.
    let mut bytes = [0; 58];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2] = 58;
    bytes[10] = 54;
    bytes[14] = 40;
    bytes[18] = 1;
    bytes[22] = 1;
    bytes[26] = 1;
    bytes[28] = 24;
    bytes[34] = 4;
    bytes[54] = 255;
    fs::write(path, bytes).unwrap();
}
