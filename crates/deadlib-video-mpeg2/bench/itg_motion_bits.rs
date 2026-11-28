// Appended only to temporary bitstream.rs copies for motion capture/replay.
impl<'a> Bits<'a> {
    #[cfg(itg_motion_capture)]
    pub(crate) fn perf_motion_context(&self) -> ([u8; 8], u8, u8) {
        let tail = &self.bytes[self.position / 8..];
        let len = tail.len().min(8);
        let mut bytes = [0; 8];
        bytes[..len].copy_from_slice(&tail[..len]);
        (bytes, len as u8, (self.position % 8) as u8)
    }

    pub(crate) fn perf_motion_from_context(bytes: &'a [u8], offset: u8) -> Self {
        Self {
            bytes,
            position: usize::from(offset),
        }
    }

    pub(crate) fn perf_motion_position(&self) -> usize {
        self.position
    }
}
