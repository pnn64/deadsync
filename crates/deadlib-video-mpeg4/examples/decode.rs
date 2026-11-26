use std::{
    env, fs,
    io::{self, BufWriter, Write},
    sync::Arc,
};

use deadlib_video_mpeg4::{decode_avi, decode_elementary_stream, Frame};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let (Some(format), Some(input), Some(output), None) =
        (args.next(), args.next(), args.next(), args.next())
    else {
        return Err("usage: decode <avi|es> <input> <output.yuv|->".into());
    };
    let decoder = match format.to_str() {
        Some("avi") => decode_avi,
        Some("es") => decode_elementary_stream,
        _ => return Err("input format must be avi or es".into()),
    };
    let bytes = fs::read(input)?;
    let out: Box<dyn Write> = if output == "-" {
        Box::new(io::stdout().lock())
    } else {
        Box::new(fs::File::create(output)?)
    };
    let mut writer = BufWriter::with_capacity(64 * 1024, out);
    let mut written = 0;
    let mut write_result = Ok(());
    let mut write_frame = |frame: Arc<Frame>| {
        if write_result.is_ok() {
            write_result = write_yuv(&mut writer, &frame);
            if write_result.is_ok() {
                written += 1;
            }
        }
    };
    decoder(&bytes, &mut write_frame)?;
    write_result?;
    writer.flush()?;
    eprintln!("Wrote {written} frames.");
    Ok(())
}

fn write_yuv(writer: &mut impl Write, frame: &Frame) -> io::Result<()> {
    for plane in frame.planes() {
        for row in plane.rows() {
            writer.write_all(row)?;
        }
    }
    Ok(())
}
