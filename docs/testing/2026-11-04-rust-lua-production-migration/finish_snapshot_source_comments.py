"""Replace staging comments after materializing the snapshot implementation."""
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
paths=[r/'crates/deadsync-song-lua/src/rust_engine/actors'/name for name in [
    'input_snapshot.rs','input_snapshot/tables.rs','input_snapshot/update.rs','lifecycle/snapshot.rs']]
for p in paths:
    source=p.read_text()
    source=source.replace('// Draft only: local, non-executing inspection for the existing snapshot schema.',
                          '// Local, non-executing inspection for the existing snapshot schema.')
    source=source.replace('// Draft only. The caller gates playback/capture/hooks and restores the complete',
                          '// The caller gates playback/capture/hooks and restores the complete')
    source=source.replace('//! Draft port of the existing immutable dense Boolean-table array schema.',
                          '//! The existing immutable dense Boolean-table array schema.')
    source=source.replace('// Draft only: the source jobs must come from the existing cached lifecycle',
                          '// Source jobs come from the existing cached lifecycle')
    source=source.replace('// Draft for a child module of actors::lifecycle. Reuse the cached FrameNode',
                          '// Reuse the cached FrameNode')
    source=source.replace('// FrameNode needs two frozen native-job flags: recurring and texture. Capture',
                          '// FrameNode retains native recurring and texture job flags. Capture')
    p.write_text(source)
