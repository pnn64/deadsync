//! Stream/PCM audit utility; does not write audio or change playback settings.
use deadlib_audio_decode_ogg::PageReader;
use deadlib_audio_decode_vorbis::{Error, OggDecoder, OggEvent};
use std::{fs::File, io::BufReader};

fn inspect(path: &std::ffi::OsStr) -> Result<(), Error> {
    let file = File::open(path).map_err(deadlib_audio_decode_ogg::Error::from)?;
    let mut pages = PageReader::new(BufReader::new(file));
    let mut decoder = OggDecoder::default();
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
    result
}
fn main() {
    let mut failed = false;
    for path in std::env::args_os().skip(1) {
        if inspect(&path).is_err() {
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}
