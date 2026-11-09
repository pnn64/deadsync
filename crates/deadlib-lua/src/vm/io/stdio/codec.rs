//! Allocation-free console encoding state, tested independently of OS handles.
use std::io;

pub(super) struct Input {
    bytes: [u8; 12288],
    start: usize,
    end: usize,
    surrogate: u16,
}
impl Input {
    pub(super) const fn new() -> Self {
        Self {
            bytes: [0; 12288],
            start: 0,
            end: 0,
            surrogate: 0,
        }
    }
    pub(super) fn read_with(
        &mut self,
        output: &mut [u8],
        mut receive: impl FnMut(&mut [u16]) -> io::Result<usize>,
    ) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        while self.start == self.end {
            let mut units = [0u16; 4096];
            let prefix = usize::from(self.surrogate != 0);
            units[0] = self.surrogate;
            let count = receive(&mut units[prefix..])?;
            self.surrogate = 0;
            let mut end = prefix + count;
            if end != 0 && units[end - 1] == 26 {
                end -= 1;
            }
            let surrogate = if end != 0 && (0xd800..=0xdbff).contains(&units[end - 1]) {
                end -= 1;
                units[end]
            } else {
                0
            };
            self.start = 0;
            self.end = 0;
            let mut written = 0;
            for character in char::decode_utf16(units[..end].iter().copied()) {
                let character =
                    character.map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
                written += character.encode_utf8(&mut self.bytes[written..]).len();
            }
            self.surrogate = surrogate;
            self.end = written;
            if written == 0 && (count == 0 || surrogate == 0) {
                return Ok(0);
            }
        }
        let count = output.len().min(self.end - self.start);
        output[..count].copy_from_slice(&self.bytes[self.start..self.start + count]);
        self.start += count;
        Ok(count)
    }
}

pub(super) struct Output {
    pending: [u8; 4],
    length: usize,
}
impl Output {
    pub(super) const fn new() -> Self {
        Self {
            pending: [0; 4],
            length: 0,
        }
    }
    pub(super) fn write(
        &mut self,
        bytes: &[u8],
        mut emit: impl FnMut(&str) -> io::Result<()>,
    ) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.length != 0 {
            let mut consumed = 0;
            while consumed < bytes.len() {
                self.pending[self.length] = bytes[consumed];
                self.length += 1;
                consumed += 1;
                match std::str::from_utf8(&self.pending[..self.length]) {
                    Ok(text) => {
                        self.length = 0;
                        emit(text)?;
                        return Ok(consumed);
                    }
                    Err(error) if error.error_len().is_none() => {}
                    Err(_) => {
                        self.length = 0;
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                }
            }
            return Ok(consumed);
        }
        let candidate = &bytes[..bytes.len().min(4096)];
        let text = match std::str::from_utf8(candidate) {
            Ok(text) => text,
            Err(error) if error.valid_up_to() != 0 => {
                std::str::from_utf8(&candidate[..error.valid_up_to()]).unwrap()
            }
            Err(error) if error.error_len().is_none() => {
                self.pending[..candidate.len()].copy_from_slice(candidate);
                self.length = candidate.len();
                return Ok(candidate.len());
            }
            Err(_) => return Err(io::ErrorKind::InvalidData.into()),
        };
        emit(text)?;
        Ok(text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocation_probe::probe;

    #[test]
    fn input_keeps_split_surrogates_and_small_utf8_reads_without_allocating() {
        let expected = "aé音𝄞z\r\n".as_bytes();
        let units: Vec<u16> = "aé音𝄞z\r\n".encode_utf16().collect();
        for source_width in 1..=units.len() {
            for output_width in 1..=8 {
                let mut input = Input::new();
                let mut source = units.as_slice();
                let mut bytes = [0; 64];
                let (result, stats) = probe(1, || -> io::Result<usize> {
                    let mut total = 0;
                    loop {
                        let count =
                            input.read_with(&mut bytes[total..total + output_width], |out| {
                                let count = source.len().min(source_width).min(out.len());
                                out[..count].copy_from_slice(&source[..count]);
                                source = &source[count..];
                                Ok(count)
                            })?;
                        if count == 0 {
                            break;
                        }
                        total += count;
                    }
                    Ok(total)
                });
                assert_eq!(stats.attempts, 0);
                assert_eq!(&bytes[..result.unwrap()], expected);
                assert!(source.is_empty());
            }
        }
    }

    #[test]
    fn output_keeps_split_utf8_and_complete_prefixes_without_allocating() {
        let source = "aé音𝄞z\r\n".as_bytes();
        for width in 1..=source.len() {
            let mut output = Output::new();
            let mut bytes = [0; 64];
            let (result, stats) = probe(1, || -> io::Result<usize> {
                let mut total = 0;
                for mut chunk in source.chunks(width) {
                    while !chunk.is_empty() {
                        let count = output.write(chunk, |text| {
                            bytes[total..total + text.len()].copy_from_slice(text.as_bytes());
                            total += text.len();
                            Ok(())
                        })?;
                        chunk = &chunk[count..];
                    }
                }
                Ok(total)
            });
            assert_eq!(stats.attempts, 0);
            assert_eq!(&bytes[..result.unwrap()], source);
            assert_eq!(output.length, 0);
        }
    }

    #[test]
    fn invalid_console_encodings_allow_later_calls() {
        let mut input = Input::new();
        let mut output = Output::new();
        let mut bytes = [0; 8];
        let (result, stats) = probe(
            1,
            || -> io::Result<(io::ErrorKind, io::ErrorKind, usize)> {
                let read_error = input
                    .read_with(&mut bytes, |out| {
                        out[..2].copy_from_slice(&[b'a' as u16, 0xdc00]);
                        Ok(2)
                    })
                    .unwrap_err()
                    .kind();
                output.write(&[0xf0, 0x9f], |_| unreachable!())?;
                let write_error = output.write(b"x", |_| unreachable!()).unwrap_err().kind();
                let mut emitted = 0;
                output.write(b"next", |text| {
                    emitted += text.len();
                    Ok(())
                })?;
                let count = input.read_with(&mut bytes, |out| {
                    out[0] = b'B' as u16;
                    Ok(1)
                })?;
                Ok((read_error, write_error, emitted + count))
            },
        );
        assert_eq!(stats.attempts, 0);
        assert_eq!(
            result.unwrap(),
            (io::ErrorKind::InvalidData, io::ErrorKind::InvalidData, 5)
        );
        assert_eq!(bytes[0], b'B');
    }
}
