//! Stream/PCM audit utility; does not write audio or change playback settings.
use deadlib_audio_decode_ogg::PageReader;
use deadlib_audio_decode_vorbis::{
    Error, OggDecoder, OggEvent, OggOptions, OggSeekReader, SeekOptions,
};
use std::{fs::File, io::BufReader};

fn inspect(path: &std::ffi::OsStr, zero_span: bool, final_trim: bool) -> Result<(), Error> {
    let file = File::open(path).map_err(deadlib_audio_decode_ogg::Error::from)?;
    if final_trim {
        let mut reader = OggSeekReader::with_options(
            BufReader::new(file),
            SeekOptions {
                allow_zero_span_granules: zero_span,
                trim_to_final_granule: true,
                ..SeekOptions::default()
            },
        )?;
        eprintln!(
            "compatibility: {:?}",
            reader.index().container().compatibility()
        );
        let ids: Vec<_> = reader.index().streams().iter().map(|s| s.info.id).collect();
        for id in ids {
            reader.seek(id, 0)?;
            eprintln!(
                "stream {}:{}: {} channels, {} Hz, end padding {}, extended trim {}",
                id.chain,
                id.serial,
                reader.selected().info.identification.channels,
                reader.selected().info.identification.sample_rate,
                reader.selected().end_padding_frames,
                reader.selected().extended_end_trim
            );
            while reader.read_next(|event| {
                if let OggEvent::StreamEnd(end) = event {
                    eprintln!("end: {} frames", end.frames);
                }
            })? {}
        }
        return Ok(());
    }
    let mut pages = PageReader::new(BufReader::new(file));
    let mut decoder = OggDecoder::new(OggOptions {
        allow_zero_span_granules: zero_span,
        ..OggOptions::default()
    });
    let mut count = 0;
    let mut last_offset = 0;
    let mut frames = 0;
    let result = (|| {
        while let Some(page) = pages.next_page()? {
            count += 1;
            last_offset = page.offset;
            decoder.push_page(page.page, page.offset, |event| match event {
                OggEvent::StreamStart(info) => eprintln!(
                    "stream {}:{}: {} channels, {} Hz, vendor {:?}",
                    info.id.chain,
                    info.id.serial,
                    info.identification.channels,
                    info.identification.sample_rate,
                    String::from_utf8_lossy(info.metadata.vendor())
                ),
                OggEvent::Pcm(pcm) => frames += pcm.pcm.frames as u64,
                OggEvent::StreamEnd(end) => eprintln!(
                    "end {}:{}: {} frames, granules {}..{}",
                    end.stream.chain,
                    end.stream.serial,
                    end.frames,
                    end.start_granule,
                    end.end_granule
                ),
                OggEvent::CommentError { error, .. } => eprintln!("comment warning: {error}"),
                OggEvent::IgnoredStream(id) => {
                    eprintln!("ignored codec {}:{}", id.chain, id.serial)
                }
            })?;
        }
        decoder.finish()
    })();
    eprintln!(
        "{}: {count} pages, {frames} emitted frames, last page offset {last_offset}: {}",
        path.to_string_lossy(),
        result
            .as_ref()
            .map_or_else(|e| e.to_string(), |_| "OK".to_owned())
    );
    eprintln!("compatibility: {:?}", decoder.compatibility());
    result
}
fn main() {
    let mut zero_span = false;
    let mut final_trim = false;
    let mut paths = Vec::new();
    for argument in std::env::args_os().skip(1) {
        match argument.to_str() {
            Some("--allow-zero-span-granules") => zero_span = true,
            Some("--trim-to-final-granule") => final_trim = true,
            _ => paths.push(argument),
        }
    }
    if paths.is_empty() {
        eprintln!(
            "usage: inspect [--allow-zero-span-granules] [--trim-to-final-granule] file.ogg ..."
        );
        std::process::exit(2);
    }
    let mut failed = false;
    for path in paths {
        if let Err(error) = inspect(&path, zero_span, final_trim) {
            eprintln!("{}: {error}", path.to_string_lossy());
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}
