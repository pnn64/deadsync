use super::*;

// The original SongFolderMedia handoff borrowed this temporary Vec and then
// dropped it. The borrowed From implementation is unchanged from the base.

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
