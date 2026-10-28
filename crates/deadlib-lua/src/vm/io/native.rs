//! Native FILE handles use their owning C image's stdio implementation.
use std::fmt::Debug;

pub(crate) trait FileBackend: Debug + Send + Sync {
    fn attach(&self, pointer: usize) -> Box<dyn File>;
}
pub(crate) trait File: Debug + Send + Sync {
    fn pointer(&self) -> usize;
    fn get(&mut self, peek: bool) -> Result<Option<u8>, i32>;
    fn read(&mut self, bytes: &mut [u8]) -> Result<usize, i32>;
    fn line(&mut self, bytes: &mut Vec<u8>) -> Result<bool, i32>;
    fn number(&mut self) -> Result<Option<f64>, i32>;
    fn write(&mut self, bytes: &[u8]) -> Result<(), i32>;
    fn flush(&mut self) -> Result<(), i32>;
    fn seek(&mut self, mode: usize, offset: i64) -> Result<i64, i32>;
    fn buffering(&mut self, mode: u8, size: usize) -> Result<(), i32>;
    fn clear_error(&mut self);
    fn close(&mut self, pipe: bool) -> Result<(), i32>;
    fn display(&self) -> Vec<u8>;
}
