#![no_main]
use deadlib_image_gif::*;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let options=DecodeOptions {limits:Limits {max_input_bytes:1<<20,max_frame_pixels:1<<16,
        max_total_pixels:1<<18,max_metadata_bytes:1<<16,max_blocks:4096,max_frames:64},..Default::default()};
    let actual=decode_with_options(bytes,&options);
    let mut decoder=Decoder::new(options);
    let mut at=0;let mut blocks=Vec::new();let mut header=None;let mut failed=false;
    let chunk=1+bytes.first().copied().unwrap_or(0) as usize;
    while at<bytes.len() {
        let end=(at+chunk).min(bytes.len());
        match decoder.update(&bytes[at..end]) {
            Err(_)=>{failed=true;break;},
            Ok(p)=>{
                assert!(p.consumed>0);at+=p.consumed;
                match p.event {
                    Some(Event::Header)=>header=decoder.header().cloned(),
                    Some(Event::Block(b))=>blocks.push(b),
                    Some(Event::End)=>break,
                    None=>{}
                }
            }
        }
    }
    let streaming_ok=!failed && at==bytes.len() && decoder.finish().is_ok();
    assert_eq!(actual.is_ok(),streaming_ok);
    let Ok(actual)=actual else {return;};
    assert_eq!(header.as_ref(),Some(&actual.header));assert_eq!(blocks,actual.blocks);
    let encoded=encode(&actual,&EncodeOptions::default()).unwrap();
    assert_eq!(decode(&encoded).unwrap(),actual);
    if let Ok(mut compositor)=Compositor::new(&actual.header) {
        for block in &actual.blocks {let _=compositor.render_block(block);}
    }
    // Most reference decoders ignore plain-text graphics and their control scope.
    if actual.blocks.iter().any(|b|matches!(b,Block::Extension(Extension::PlainText(_)))) {return;}
    let mut opts=gif::DecodeOptions::new();opts.set_color_output(gif::ColorOutput::Indexed);
    opts.set_memory_limit(gif::MemoryLimit::Bytes(std::num::NonZeroU64::new(1<<20).unwrap()));
    let Ok(mut reference)=opts.read_info(bytes) else {return;};
    for frame in actual.frames() {
        let Ok(Some(f))=reference.read_next_frame() else {return;};
        assert_eq!((f.left,f.top,f.width,f.height),(frame.left,frame.top,frame.width,frame.height));
        assert_eq!(f.buffer.as_ref(),frame.indices);
    }
});
