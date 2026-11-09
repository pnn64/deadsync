"""Retain the preceding compiler regression scope and add gameplay callbacks."""
import ast, subprocess, sys
from pathlib import Path
d=Path(__file__).parent
tree=ast.parse((d/'run_live_compiler_regressions.py').read_text())
targets=next(ast.literal_eval(n.value) for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='targets' for t in n.targets))
targets=['rust_live_gameplay',*targets]
cmd=[sys.executable,str(d/'checks.py'),'song-live-gameplay-regressions-v1','song','--target-dir','/var/tmp/deadsync-song-rust-production-20261107','--features','rust-engine,test-support','--no-fail-fast','--jobs','4']
for t in targets:cmd+=['--test',t]
raise SystemExit(subprocess.call(cmd))
