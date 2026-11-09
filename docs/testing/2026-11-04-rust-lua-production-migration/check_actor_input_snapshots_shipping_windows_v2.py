"""Keep the first shipping check immutable while recording the lint cleanup."""
from pathlib import Path
p = Path(__file__).with_name('check_actor_input_snapshots_shipping_windows.py')
source = p.read_text()
assert source.count('actor-input-snapshots-shipping-windows-v1') == 1
exec(compile(source.replace('actor-input-snapshots-shipping-windows-v1',
                            'actor-input-snapshots-shipping-windows-v2'), str(p) + ':v2', 'exec'))
