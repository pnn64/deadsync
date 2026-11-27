use std::{
    env, fs,
    io::{self, BufWriter, Write},
    path::Path,
};

use deadlib_video_vp9::{decode_mp4_pictures, ivf::IvfReader, Decoder, Picture};

fn write_frame(output: &mut impl Write, frame: &Picture) -> io::Result<()> {
    for plane in frame.planes() {
        if let Some(rows) = plane.rows_u8() {
            for row in rows {
                output.write_all(row)?;
            }
        } else if let Some(rows) = plane.rows_u16() {
            for row in rows {
                for sample in row {
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
        return Err("usage: decode <video.mp4|video.ivf> <output.yuv|->".into());
    };
    let input = fs::read(input)?;
    let file: Box<dyn Write> = if output == "-" {
        Box::new(io::stdout().lock())
    } else {
        Box::new(fs::File::create(Path::new(output))?)
    };
    let mut file = BufWriter::new(file);
    let mut count = 0;
    if input.starts_with(b"DKIF") {
        let packets = IvfReader::new(&input)?;
        if &packets.header().fourcc != b"VP90" {
            return Err("expected VP90 IVF input".into());
        }
        let mut decoder = Decoder::new();
        for packet in packets {
            if let Some(frame) = decoder.push_picture(packet?.data)? {
                write_frame(&mut file, &frame)?;
                count += 1;
            }
        }
    } else {
        let mut result = Ok(());
        let decode_result = decode_mp4_pictures(&input, |frame| {
            if result.is_ok() {
                result = write_frame(&mut file, &frame);
                count += 1;
            }
        });
        result?;
        decode_result?;
    }
    file.flush()?;
    eprintln!("Wrote {count} frames.");
    Ok(())
}
