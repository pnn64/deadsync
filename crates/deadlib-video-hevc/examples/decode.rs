use std::{
    env, fs,
    io::{self, BufWriter, Write},
    path::Path,
};

use deadlib_video_hevc::{Frame, PixelData, decode_annex_b, decode_mp4};

fn write_frame(output: &mut impl Write, frame: Frame) -> io::Result<()> {
    for plane in [frame.y, frame.u, frame.v] {
        match plane {
            PixelData::U8(samples) => output.write_all(&samples)?,
            PixelData::U16(samples) => {
                for sample in samples {
                    output.write_all(&sample.to_le_bytes())?;
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [input, output] = args.as_slice() else {
        return Err("usage: decode <video.mp4|video.hevc> <output.yuv|->".into());
    };
    let from_mp4 = Path::new(input)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"));
    let input = fs::read(input)?;
    let output: Box<dyn Write> = if output == "-" {
        Box::new(io::stdout().lock())
    } else {
        Box::new(fs::File::create(output)?)
    };
    let mut output = BufWriter::with_capacity(64 * 1024, output);
    let mut result = Ok(());
    let mut count = 0;
    let mut emit = |frame| {
        if result.is_ok() {
            result = write_frame(&mut output, frame);
            count += 1;
        }
    };
    if from_mp4 {
        decode_mp4(&input, &mut emit)?;
    } else {
        decode_annex_b(&input, &mut emit)?;
    }
    result?;
    output.flush()?;
    eprintln!("Wrote {count} frames.");
    Ok(())
}
