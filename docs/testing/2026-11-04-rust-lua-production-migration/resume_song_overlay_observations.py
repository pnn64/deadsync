from pathlib import Path
p=Path(__file__).with_name('hook_song_overlay_observations.py')
s=p.read_text(); head=s[:s.index("edit('src/rust_engine/actors.rs'")]
tail=s[s.index("edit('src/rust_engine/actors/capture.rs'"):]
head += "\noriginal_edit = edit\ndef edit(name, old, new):\n if name == 'src/rust_engine/actors/capture.rs' and old.startswith('    if !song_pose::record'):\n  p=r/name; b=p.read_bytes(); nl='\\r\\n' if b'\\r\\n' in b else '\\n'; old=old.replace('\\n',nl).encode(); new=new.replace('\\n',nl).encode(); assert b.count(old)==2; p.write_bytes(b.replace(old,new))\n else: original_edit(name,old,new)\n"
exec(compile(head+tail,str(p)+':resume','exec'))
