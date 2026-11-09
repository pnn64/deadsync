"""The fixed Lua migration input scope; retain full workspace guards separately."""
from pathlib import Path
import json

PACKAGES = ('deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin',
            'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell',
            'deadsync-notefield')

def inputs(snapshot):
    inventory = json.loads((Path(__file__).resolve().parent/'inventory.json').read_text())
    original = {row['file'].replace('\\','/') for row in inventory['source_references']}
    prefixes = tuple('crates/'+name+'/' for name in PACKAGES)
    return {name:digest for name,digest in snapshot.items()
            if name.replace('\\','/') in original
            or name.startswith(prefixes)
            or name.startswith('src/')
            or name in {'Cargo.toml','Cargo.lock','.cargo/config.toml','build.rs'}
            or name.endswith('/Cargo.toml')}
