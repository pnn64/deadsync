use deadlib_audio_decode_ogg::{Demuxer, PageReader};
use deadlib_audio_decode_vorbis::{bitpack::BitReader, *};
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};
use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
};

pub fn headers(bytes: &[u8]) -> Vec<[Vec<u8>; 3]> {
    let mut reader = PageReader::new(Cursor::new(bytes));
    let mut demux = Demuxer::default();
    let mut streams: BTreeMap<u32, [Vec<u8>; 3]> = BTreeMap::new();
    while let Some(located) = reader.next_page().unwrap() {
        demux
            .push_page(located.page, located.offset, |p| {
                if p.info.number < 3 {
                    streams.entry(p.info.serial).or_default()[p.info.number as usize] =
                        p.data.to_vec();
                }
            })
            .unwrap();
    }
    demux.finish().unwrap();
    streams.into_values().collect()
}
fn check(bytes: &[u8]) -> (usize, usize) {
    let packets = headers(bytes);
    assert_eq!(packets.len(), 1, "single-stream corpus helper");
    let identification = Identification::parse(&packets[0][0]).unwrap();
    let comments = Comments::parse(&packets[0][1], &Limits::default()).unwrap();
    assert!(!comments.vendor.is_empty());
    let setup = Setup::parse(&packets[0][2], &identification, &Limits::default()).unwrap();
    let mss = MediaSourceStream::new(
        Box::new(Cursor::new(bytes.to_vec())),
        MediaSourceStreamOptions::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let parameters = format
        .tracks()
        .iter()
        .find_map(|t| t.codec_params.as_ref()?.audio())
        .unwrap();
    assert_eq!(parameters.sample_rate, Some(identification.sample_rate));
    assert_eq!(
        parameters.channels.as_ref().unwrap().count(),
        identification.channels as usize
    );
    // Symphonia independently parses the complete setup while constructing its decoder.
    symphonia::default::get_codecs()
        .make_audio_decoder(parameters, &AudioDecoderOptions::default())
        .unwrap();
    let mut reader = PageReader::new(Cursor::new(bytes));
    let mut demux = Demuxer::default();
    let mut audio = 0;
    while let Some(located) = reader.next_page().unwrap() {
        demux
            .push_page(located.page, located.offset, |p| {
                if p.info.number >= 3 {
                    let value = setup
                        .read_audio_header(&mut BitReader::new(p.data))
                        .unwrap();
                    assert!(identification.block_sizes.contains(&value.block_size));
                    audio += 1;
                }
            })
            .unwrap();
    }
    demux.finish().unwrap();
    (setup.codebooks().len(), audio)
}

#[test]
fn generated_stereo_and_eight_channel_headers_match_symphonia() {
    for bytes in [
        include_bytes!("fixtures/stereo.ogg").as_slice(),
        include_bytes!("fixtures/eight-channel.ogg").as_slice(),
    ] {
        let (books, audio) = check(bytes);
        assert!(books > 1);
        assert!(audio > 1);
    }
}
fn collect(root: &Path, paths: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            collect(&entry.path(), paths);
        } else if entry
            .path()
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ogg"))
        {
            paths.push(entry.path());
        }
    }
}
#[test]
#[ignore = "requires DEADLIB_VORBIS_CORPUS directory of single-stream Ogg Vorbis files"]
fn local_corpus_headers_match_symphonia() {
    let root = std::env::var_os("DEADLIB_VORBIS_CORPUS").expect("set DEADLIB_VORBIS_CORPUS");
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(!paths.is_empty());
    let (mut books, mut audio) = (0, 0);
    for path in &paths {
        eprintln!("{}", path.display());
        let result = check(&fs::read(path).unwrap());
        books += result.0;
        audio += result.1;
    }
    eprintln!(
        "Symphonia setup/header parity: {} files, {books} codebooks, {audio} audio packet prefixes",
        paths.len()
    );
}
