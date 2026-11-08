//! Reference-only canonical metadata dump. Text stays bytes (hexadecimal).
use deadlib_audio_decode_wav::{
    Limits, Wave,
    metadata::{Metadata, nul_terminated},
};
fn hex(b: &[u8]) -> String {
    if b.is_empty() {
        "-".into()
    } else {
        b.iter().map(|v| format!("{v:02x}")).collect()
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: metadata_fields INPUT")?;
    let bytes = std::fs::read(path)?;
    let wave = Wave::parse(&bytes)?;
    let e = wave.format.endian;
    for chunk in wave.chunks()? {
        let chunk = chunk?;
        match chunk.metadata(e, Limits::default())? {
            Metadata::Bext(b) => {
                let umid = b.umid().map_or_else(|| "-".into(), |v| hex(v));
                let loud = b.loudness().map_or_else(
                    || "-".into(),
                    |v| {
                        format!(
                            "{},{},{},{},{}",
                            v.integrated, v.range, v.true_peak, v.momentary, v.short_term
                        )
                    },
                );
                println!(
                    "bext {} {} {} {} {} {} {} {} {} {}",
                    b.version(),
                    b.time_reference(),
                    hex(b.description()),
                    hex(b.originator()),
                    hex(b.originator_reference()),
                    hex(b.date()),
                    hex(b.time()),
                    umid,
                    loud,
                    hex(b.coding_history())
                );
            }
            Metadata::Cues(c) => {
                for p in c.iter() {
                    println!(
                        "cue {} {} {} {} {} {}",
                        p.id,
                        p.position,
                        hex(&p.data_chunk),
                        p.chunk_start,
                        p.block_start,
                        p.sample_offset
                    );
                }
            }
            Metadata::Sampler(s) => {
                let h = s.header();
                println!(
                    "smpl {} {} {} {} {} {} {} {} {}",
                    h.manufacturer,
                    h.product,
                    h.sample_period,
                    h.midi_unity_note,
                    h.midi_pitch_fraction,
                    h.smpte_format,
                    h.smpte_offset,
                    s.loops().len(),
                    hex(s.specific_data())
                );
                for p in s.loops() {
                    println!(
                        "loop {} {} {} {} {} {}",
                        p.cue_id, p.kind, p.start, p.end, p.fraction, p.play_count
                    );
                }
            }
            Metadata::List(list) => {
                for entry in list.entries() {
                    let entry = entry?;
                    if list.kind() == *b"INFO" && [b"INAM", b"IART"].contains(&&entry.id) {
                        println!(
                            "info {} {}",
                            std::str::from_utf8(&entry.id)?,
                            hex(entry.text_bytes())
                        );
                    }
                    if list.kind() == *b"adtl" && [*b"labl", *b"note", *b"ltxt"].contains(&entry.id)
                    {
                        let a = entry.associated(e)?;
                        if let Some(length) = a.sample_length {
                            let locale = a.locale.unwrap();
                            println!(
                                "ltxt {} {} {} {} {} {} {} {}",
                                a.cue_id,
                                length,
                                hex(&a.purpose.unwrap()),
                                locale[0],
                                locale[1],
                                locale[2],
                                locale[3],
                                hex(nul_terminated(a.text))
                            );
                        } else {
                            println!(
                                "{} {} {}",
                                std::str::from_utf8(&entry.id)?,
                                a.cue_id,
                                hex(nul_terminated(a.text))
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
