//! Shared song VM ownership for synchronous gameplay callbacks and reloads.
use super::*;
use std::sync::MutexGuard;

/// A song session's one VM. Callers release its guard before applying gameplay
/// writes, which can synchronously invoke another gameplay callback.
#[derive(Clone)]
pub struct SongLiveRuntime {
    runtime: Arc<Mutex<RustSongRuntime>>,
    releases: Arc<Mutex<Vec<Root>>>,
}
impl SongLiveRuntime {
    pub(super) fn new(runtime: RustSongRuntime) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
            releases: Default::default(),
        }
    }
    /// Enter the session between Lua calls. Host callbacks already executing in
    /// this VM use HostCall for nested Lua calls rather than reacquiring it.
    pub fn access(&self) -> MutexGuard<'_, RustSongRuntime> {
        let mut runtime = self.runtime.lock().expect("song VM");
        for root in self.releases.lock().expect("song roots").drain(..) {
            runtime.vm.unpin(root).expect("owned song root");
        }
        runtime
    }
    pub(super) fn retain(
        &self,
        runtime: &mut RustSongRuntime,
        value: Value,
    ) -> Result<Retained, Error> {
        Ok(Retained {
            root: runtime.vm.pin(value)?,
            releases: self.releases.clone(),
        })
    }
}
/// Destruction only queues a release. It cannot run Lua, acquire the VM lock,
/// or create a reference cycle from a Lua HostFn back to its VM.
pub(super) struct Retained {
    pub(super) root: Root,
    releases: Arc<Mutex<Vec<Root>>>,
}
impl Retained {
    pub(super) fn value(&self, runtime: &RustSongRuntime) -> Result<Value, Error> {
        runtime.vm.rooted(self.root)
    }
}
impl Drop for Retained {
    fn drop(&mut self) {
        self.releases.lock().expect("song roots").push(self.root);
    }
}
