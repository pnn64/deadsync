//! File destinations and exact vector/stream comparisons for the oracle CLI.
use deadlib_image_jpeg::{
    AbbreviatedBytes, AbbreviatedImage, AbbreviatedStreamEncoder, AbbreviatedWriteResult,
};
use std::{
    fs::File,
    io::{self, Write},
    path::Path,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Default)]
pub struct Destination(Option<File>);
impl Write for Destination {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0
            .as_mut()
            .ok_or(io::ErrorKind::BrokenPipe)?
            .write(data)
    }
    fn flush(&mut self) -> io::Result<()> {
        panic!("the codec must not flush the destination")
    }
}
pub struct Probe {
    encoder: AbbreviatedStreamEncoder<Destination, Destination>,
    requested: usize,
}
impl Probe {
    pub fn new(bytes: usize) -> Self {
        Self {
            encoder: AbbreviatedStreamEncoder::with_buffer_size(
                Destination::default(),
                Destination::default(),
                bytes,
            ),
            requested: bytes,
        }
    }
    pub fn compare(
        &mut self,
        prefix: &Path,
        expected: &AbbreviatedImage,
        encode: impl FnOnce(
            &mut AbbreviatedStreamEncoder<Destination, Destination>,
        ) -> AbbreviatedWriteResult<AbbreviatedBytes>,
    ) -> Result<()> {
        let append = |suffix: &str| {
            let mut name = prefix.as_os_str().to_os_string();
            name.push(suffix);
            std::path::PathBuf::from(name)
        };
        let table_path = append(".tables.jpg");
        let image_path = append(".jpg");
        self.encoder.get_mut().0.0 = Some(File::create(&table_path)?);
        self.encoder.get_mut().1.0 = Some(File::create(&image_path)?);
        let count = encode(&mut self.encoder)?;
        self.encoder.get_mut().0.0.take();
        self.encoder.get_mut().1.0.take();
        assert_eq!(
            (count.tables, count.image),
            (expected.tables.len(), expected.image.len())
        );
        assert!(self.encoder.buffer_capacity() <= self.requested);
        assert_eq!(std::fs::read(table_path)?, expected.tables);
        assert_eq!(std::fs::read(image_path)?, expected.image);
        Ok(())
    }
}
