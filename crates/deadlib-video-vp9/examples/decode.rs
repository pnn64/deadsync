use std::{
    env, fs,
    io::{self, BufWriter, Write},
    path::Path,
};

use deadlib_video_vp9::{decode_mp4, ivf::IvfReader, Decoder, Frame, PlaneData};

fn write_frame(output: &mut impl Write, frame: &Frame) -> io::Result<()> {
    for plane in [&frame.y, &frame.u, &frame.v] {
        match plane {
            PlaneData::U8(samples) => output.write_all(samples)?,
            PlaneData::U16(samples) => {
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
            if let Some(frame) = decoder.push_packet(packet?.data)? {
                write_frame(&mut file, &frame)?;
                count += 1;
            }
        }
    } else {
        let mut result = Ok(());
        let decode_result = decode_mp4(&input, |frame| {
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
