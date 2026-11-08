use super::*;
use crate::metadata_perf::{compare_owned, measure};
use std::hint::black_box;

// The original SongFolderMedia handoff borrowed this temporary Vec and then
// dropped it. The borrowed From implementation is unchanged from the base.
fn original_handoff(changes: Vec<SongBackgroundChange>) -> Vec<SerializableSongBackgroundChange> {
    changes
        .iter()
        .map(SerializableSongBackgroundChange::from)
        .collect()
}

fn change(index: usize, kind: usize) -> SongBackgroundChange {
    SongBackgroundChange {
        start_beat: index as f32 * 0.5,
        target: match kind % 4 {
            0 => SongBackgroundChangeTarget::File(PathBuf::from(format!(
                "Songs/Pack/Song/visual-{index}.mp4"
            ))),
            1 => SongBackgroundChangeTarget::Animation(format!("Animation-{index}-\u{00e9}")),
            2 => SongBackgroundChangeTarget::NoSongBg,
            _ => SongBackgroundChangeTarget::Random,
        },
        rate: 1.25,
        effect: "StretchNormal".to_owned(),
        file2: Some(PathBuf::from(format!("Songs/Pack/Song/second-{index}.png"))),
        transition: "CrossFade".to_owned(),
        color1: Some([f32::from_bits(0x7fc01234), -0.0, f32::INFINITY, 1.0]),
        color2: None,
    }
}

fn encode(changes: &[SerializableSongBackgroundChange]) -> Vec<u8> {
    bincode::encode_to_vec(changes, bincode::config::standard()).unwrap()
}

#[test]
fn owned_background_handoff_preserves_serialized_fields_and_float_bits() {
    let mut input: Vec<_> = (0..16).map(|i| change(i, i)).collect();
    input.reserve(128);
    input[0].effect.reserve(1024);
    input[0].transition.reserve(1024);
    input[0].file2.as_mut().unwrap().reserve(1024);
    input[1].file2 = None;
    input[2].effect.clear();
    input[3].transition.clear();
    input[4].start_beat = f32::from_bits(0xffc05678);
    input[5].rate = -0.0;
    input[6].color2 = Some([0.1, 0.2, 0.3, 0.4]);
    let expected = original_handoff(input.clone());
    let actual = cache_background_changes(input);
    assert_eq!(encode(&actual), encode(&expected));
    assert!(cache_background_changes(Vec::new()).is_empty());
    assert_eq!(
        cache_background_changes(Vec::with_capacity(32)).capacity(),
        0
    );
}

#[test]
fn owned_background_paths_keep_lossy_os_string_behavior() {
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_wide(&[
            b'A' as u16,
            0xd800,
            b'B' as u16,
        ]))
    };
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(vec![b'A', 0xff, b'B']))
    };
    #[cfg(any(windows, unix))]
    {
        let mut input = change(0, 0);
        input.target = SongBackgroundChangeTarget::File(path.clone());
        input.file2 = Some(path);
        let expected = SerializableSongBackgroundChange::from(&input);
        let actual = SerializableSongBackgroundChange::from(input);
        assert_eq!(encode(&[actual]), encode(&[expected]));
    }
}

#[test]
fn owned_background_handoff_reuses_text_and_path_storage() {
    let input = change(0, 0);
    let effect_ptr = input.effect.as_ptr();
    let transition_ptr = input.transition.as_ptr();
    let SongBackgroundChangeTarget::File(path) = &input.target else {
        unreachable!()
    };
    let path_ptr = path.as_os_str().as_encoded_bytes().as_ptr();
    let file2_ptr = input
        .file2
        .as_ref()
        .unwrap()
        .as_os_str()
        .as_encoded_bytes()
        .as_ptr();
    let (output, counts) = measure(|| SerializableSongBackgroundChange::from(input));
    let SerializableSongBackgroundChangeTarget::File(path) = &output.target else {
        unreachable!()
    };
    assert_eq!(output.effect.as_ptr(), effect_ptr);
    assert_eq!(output.transition.as_ptr(), transition_ptr);
    assert_eq!(path.as_ptr(), path_ptr);
    assert_eq!(output.file2.as_ref().unwrap().as_ptr(), file2_ptr);
    assert_eq!(counts.allocs, 0);
    assert_eq!(counts.reallocs, 0);

    let input: Vec<_> = (0..64).map(|i| change(i, 0)).collect();
    let original = input.clone();
    let (_, before) = measure(|| original_handoff(original));
    let (_, after) = measure(|| cache_background_changes(input));
    assert_eq!(before.allocs, 257);
    // The collection may allocate or resize once; no per-field allocation remains.
    assert!(after.allocs + after.reallocs <= 1);
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_library_background_handoff() {
    for count in [0, 1, 32, 1024] {
        for kind in [0, 1, 2] {
            let input: Vec<_> = (0..count)
                .map(|i| change(i, if kind == 2 { i } else { kind }))
                .collect();
            let label = format!("background/{count}/{kind}");
            let old = input.clone();
            let (_, before) = measure(|| original_handoff(old));
            let new = input.clone();
            let (_, after) = measure(|| cache_background_changes(new));
            println!("ALLOC {label}: original {before:?}, current {after:?}");
            compare_owned(
                &label,
                if count == 0 {
                    32768
                } else {
                    (2048 / count).max(2)
                },
                &input,
                |input| {
                    black_box(original_handoff(black_box(input)));
                },
                |input| {
                    black_box(cache_background_changes(black_box(input)));
                },
            );
        }
    }
}
