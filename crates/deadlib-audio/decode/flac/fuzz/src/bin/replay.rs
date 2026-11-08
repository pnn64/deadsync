use deadlib_audio_decode_flac_fuzz as harness;
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let target = args[0].to_str().unwrap();
    let path = std::path::Path::new(&args[1]);
    if target == "export" {
        let data = std::fs::read(path).unwrap();
        let (options, pcm) = harness::encode_input(&data).unwrap();
        let mut encoder =
            deadlib_audio_decode_flac::Encoder::new(std::io::Cursor::new(Vec::new()), options)
                .unwrap();
        encoder.write_interleaved(&pcm).unwrap();
        std::fs::write(
            &args[2],
            encoder.finish_seekable().unwrap().output.into_inner(),
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&args[2]).with_extension("i32le"),
            pcm.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<_>>(),
        )
        .unwrap();
        return;
    }
    let mut paths = if path.is_dir() {
        std::fs::read_dir(path)
            .unwrap()
            .map(|p| p.unwrap().path())
            .collect::<Vec<_>>()
    } else {
        vec![path.to_owned()]
    };
    paths.sort();
    let function = match target {
        "primitives" => harness::primitives,
        "frame" => harness::frame,
        "native" => harness::native,
        "ogg" => harness::ogg,
        "encode" => harness::encode,
        _ => panic!("unknown target"),
    };
    for path in paths {
        let data = std::fs::read(&path).unwrap();
        if std::panic::catch_unwind(|| function(&data)).is_err() {
            eprintln!("FAILED {}", path.display());
            std::process::exit(1);
        }
    }
}
