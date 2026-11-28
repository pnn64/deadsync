import collections
import struct
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_bytes()
assert source[:8] == b'ITGPRED1'
out = bytearray(source[:8])
offset = 8
counts = collections.Counter()
while offset < len(source):
    fields = struct.unpack_from('<13I', source, offset)
    width = fields[0]
    length = 53 + width * width + fields[8]
    record = source[offset:offset + length]
    assert len(record) == length
    counts['all'] += 1
    if fields[11] & 1 and fields[12] & 1:
        out.extend(record)
        counts['bilinear'] += 1
        counts[f'width_{width}'] += 1
        counts[f'average_{record[52]}'] += 1
    offset += length
assert offset == len(source)
target = Path(sys.argv[2])
target.parent.mkdir(parents=True, exist_ok=True)
target.write_bytes(out)
print(dict(counts))
print('filtered_trace_bytes', len(out))
