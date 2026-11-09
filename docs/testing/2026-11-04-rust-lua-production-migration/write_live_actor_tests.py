"""Create new comparisons using the existing full native live-scene harness."""
from pathlib import Path
r=Path(__file__).resolve().parents[3]
source=(r/'crates/deadsync-song-lua/tests/rust_live_frames.rs').read_text()
prefix=source[:source.index('const SNAPSHOT:')]
prefix=prefix.replace('live frame phases','live actor update order')
prefix=prefix.replace('        self.frames.finish_health_update(&mut self.rust).unwrap();', '''        assert!(self.rust.advance_song_live_actors(self.screen, frame.delta_seconds, None).unwrap());
        self.frames.finish_health_update(&mut self.rust).unwrap();''')
# Health-only tests belong to the existing host-phase suite, preserved unchanged.
a=prefix.index('    fn health(&mut self)'); b=prefix.index('\n}\nimpl Drop',a)
prefix=prefix[:a]+prefix[b:]
target=r/'crates/deadsync-song-lua/tests/rust_live_actors.rs'
assert not target.exists();target.write_text(prefix)
