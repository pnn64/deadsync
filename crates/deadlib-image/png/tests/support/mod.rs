use deadlib_image_png::{
    Error,
    zlib::{Inflater, StreamStatus},
};

pub fn inflate(
    input: &[u8],
    input_chunk: usize,
    output_chunk: usize,
    limit: usize,
) -> Result<(Vec<u8>, usize), Error> {
    assert!(input_chunk != 0 && output_chunk != 0);
    let mut inflater = Inflater::new(limit)?;
    let memory = inflater.working_memory_bytes();
    assert!(memory < 40 * 1024);
    let mut offset = 0;
    let mut result = Vec::new();
    let mut output = vec![0xcc; output_chunk];
    loop {
        output.fill(0xcc);
        let end = input.len().min(offset + input_chunk);
        let progress = inflater.process(&input[offset..end], &mut output)?;
        assert_eq!(inflater.working_memory_bytes(), memory);
        assert!(progress.consumed <= end - offset && progress.written <= output.len());
        assert!(output[progress.written..].iter().all(|&b| b == 0xcc));
        offset += progress.consumed;
        result.extend_from_slice(&output[..progress.written]);
        assert_eq!(inflater.total_in(), offset);
        assert_eq!(inflater.total_out(), result.len());
        if progress.status == StreamStatus::Done {
            inflater.finish()?;
            assert!(inflater.is_done());
            let done = inflater.process(b"unused", &mut output)?;
            assert_eq!(
                (done.consumed, done.written, done.status),
                (0, 0, StreamStatus::Done)
            );
            return Ok((result, offset));
        }
        if progress.consumed == 0 && progress.written == 0 {
            assert_eq!(offset, input.len(), "unexpected stalled inflater");
            inflater.finish()?;
            panic!("incomplete inflater finish succeeded");
        }
    }
}
