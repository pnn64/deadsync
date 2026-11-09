"""Stop only our development checker tree before correcting a reviewed gap."""
import json, os, signal
from pathlib import Path

def processes():
    result={}
    for p in Path('/proc').iterdir():
        if not p.name.isdigit():continue
        try:
            tail=(p/'stat').read_text().rsplit(')',1)[1].split()
            argv=(p/'cmdline').read_bytes().split(b'\0')
            result[int(p.name)]=(int(tail[1]),tail[19],argv)
        except (FileNotFoundError,ProcessLookupError,PermissionError):pass
    return result
table=processes()
roots=[pid for pid,(_,_,argv) in table.items() if argv and b'python' in argv[0]
       and any(a.endswith(b'/verify_song_actor_setters.py') for a in argv[1:])]
assert len(roots)==1,roots
root=roots[0];selected={root};os.kill(root,signal.SIGSTOP)
for _ in range(3):
    table=processes()
    while True:
        children={pid for pid,(parent,_,_) in table.items() if parent in selected}
        if children<=selected:break
        for pid in children-selected:
            try:os.kill(pid,signal.SIGSTOP)
            except ProcessLookupError:pass
        selected|=children
for pid in sorted(selected,reverse=True):
    try:
        current=processes().get(pid)
        if current and current[1]==table[pid][1]:os.kill(pid,signal.SIGKILL)
    except ProcessLookupError:pass
report={'reason':'Review found non-setter class fallback regression; stop before more checks, then fix and rerun.',
        'checker_pid':root,'stopped_processes':sorted(selected)}
out=Path(__file__).with_name('song-actor-setters-development-stop.json')
assert not out.exists();out.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
