//! Reference-only native PCM decoder. Integer outputs are left-aligned i32 LE;
//! floating outputs retain native width and bit patterns without arithmetic.
use std::io::Write;
use symphonia::core::{
    audio::{Audio,AudioBuffer,GenericAudioBufferRef,conv::IntoSample,sample::Sample},
    codecs::audio::AudioDecoderOptions,formats::{FormatOptions,TrackType,probe::Hint},
    io::MediaSourceStream,meta::MetadataOptions,
};
fn integers<S:Sample+IntoSample<i32>>(audio:&AudioBuffer<S>,sink:&mut impl Write)->Result<(),std::io::Error> {
    for frame in 0..audio.frames(){for channel in 0..audio.num_planes(){let value:i32=audio.plane(channel).unwrap()[frame].into_sample();sink.write_all(&value.to_le_bytes())?;}}Ok(())
}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let path=std::env::args_os().nth(1).ok_or("usage: symphonia_pcm INPUT")?;
    let mut hint=Hint::new();hint.with_extension("wav");
    let mut format=symphonia::default::get_probe().probe(&hint,MediaSourceStream::new(Box::new(std::fs::File::open(path)?),Default::default()),FormatOptions::default(),MetadataOptions::default())?;
    let track=format.default_track(TrackType::Audio).ok_or("no audio track")?;let id=track.id;
    let parameters=track.codec_params.as_ref().and_then(|p|p.audio()).ok_or("no audio parameters")?;
    let mut decoder=symphonia::default::get_codecs().make_audio_decoder(parameters,&AudioDecoderOptions::default())?;
    let mut sink=std::io::BufWriter::new(std::io::stdout().lock());
    while let Some(packet)=format.next_packet()? {
        if packet.track_id!=id {continue;}
        match decoder.decode(&packet)? {
            GenericAudioBufferRef::U8(a)=>integers(a,&mut sink)?,GenericAudioBufferRef::U16(a)=>integers(a,&mut sink)?,
            GenericAudioBufferRef::U24(a)=>integers(a,&mut sink)?,GenericAudioBufferRef::U32(a)=>integers(a,&mut sink)?,
            GenericAudioBufferRef::S8(a)=>integers(a,&mut sink)?,GenericAudioBufferRef::S16(a)=>integers(a,&mut sink)?,
            GenericAudioBufferRef::S24(a)=>integers(a,&mut sink)?,GenericAudioBufferRef::S32(a)=>integers(a,&mut sink)?,
            GenericAudioBufferRef::F32(a)=>for frame in 0..a.frames(){for channel in 0..a.num_planes(){sink.write_all(&a.plane(channel).unwrap()[frame].to_bits().to_le_bytes())?;}},
            GenericAudioBufferRef::F64(a)=>for frame in 0..a.frames(){for channel in 0..a.num_planes(){sink.write_all(&a.plane(channel).unwrap()[frame].to_bits().to_le_bytes())?;}},
        }
    }
    sink.flush()?;Ok(())
}
