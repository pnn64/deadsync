"""Test-only hierarchy marker, frame dump and independent integer helpers."""
import struct


def structure(data, require_dhp=True):
    assert data[:2] == b"\xff\xd8"
    position, frames, header, repairs, expansion = 2, [], None, [], 0
    empty_dac, empty_exp, previous = 0, 0, None
    while position < len(data):
        start = position
        assert data[position] == 255
        while data[position] == 255:
            position += 1
        marker = data[position]
        position += 1
        if marker == 0xd9:
            break
        length = int.from_bytes(data[position:position + 2], "big")
        assert length >= 2
        end = position + length
        payload = data[position + 2:end]
        if marker in (0xde, 0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf):
            p, h, w, count = payload[0], *struct.unpack_from(">HH", payload, 1), payload[5]
            assert len(payload) == 6 + 3 * count
            specs = [(payload[6 + 3 * i], payload[7 + 3 * i] >> 4, payload[7 + 3 * i] & 15) for i in range(count)]
            description = {"marker": marker, "precision": p, "width": w, "height": h, "declared_height": h, "components": specs, "scans": 0, "dnl_segments": 0}
            if marker == 0xde:
                assert header is None
                header = description
            else:
                description["expansion"] = expansion
                expansion = 0
                frames.append(description)
        if marker == 0xdc:
            assert frames and previous == 0xda and frames[-1]["scans"] == 1
            assert len(payload) >= 2 and frames[-1]["dnl_segments"] == 0
            frames[-1]["height"] = int.from_bytes(payload[:2], "big")
            assert frames[-1]["height"] > 0
            frames[-1]["dnl_segments"] += 1
        if marker == 0xdf:
            assert len(payload) == 1
            expansion = payload[0]
        if marker == 0xcc and not payload:
            repairs.append((start, end))
            empty_dac += 1
        if marker == 0xdf and payload == b"\0":
            repairs.append((start, end))
            empty_exp += 1
        if marker == 0xda:
            assert frames
            frames[-1]["scans"] += 1
            while end < len(data):
                if data[end] != 255:
                    end += 1
                    continue
                j = end + 1
                while data[j] == 255:
                    j += 1
                if data[j] == 0 or 0xd0 <= data[j] <= 0xd7:
                    end = j + 1
                else:
                    break
        position = end
        previous = marker
    assert frames
    assert all(f["height"] > 0 for f in frames)
    if header is None:
        assert not require_dhp and len(frames) == 1
        header = dict(frames[0])
    elif header["height"] == 0:
        heights = {}
        for frame in frames:
            mv = max(c[2] for c in frame["components"])
            for component_id, _, v in frame["components"]:
                heights[component_id] = (frame["height"] * v + mv - 1) // mv
        mv = max(c[2] for c in header["components"])
        final = {heights[cid] for cid, _, v in header["components"] if v == mv}
        assert len(final) == 1
        header["height"] = final.pop()
    normalized = bytearray(data)
    for start, end in reversed(repairs):
        del normalized[start:end]
    return header, frames, bytes(normalized), empty_dac, empty_exp

def samples(data):
    assert data[:4] == b"JLP1"
    p, w, h, count = struct.unpack_from("<4I", data, 4)
    position, components = 20, []
    for _ in range(count):
        shape = struct.unpack_from("<5I", data, position)
        position += 20
        length = shape[3] * shape[4]
        values = struct.unpack_from("<" + "H" * length, data, position)
        position += 2 * length
        components.append((shape, values))
    assert position == len(data)
    return p, w, h, components

def check_frame_data(frames, dumps, observations):
    """Compare ordered per-frame/component blocks or rows before reconstruction."""
    scan_types = {0xc0: 0, 0xc1: 1, 0xc2: 2, 0xc3: 3, 0xc5: 4, 0xc6: 5,
                  0xc7: 6, 0xc9: 8, 0xca: 9, 0xcb: 10, 0xcd: 11, 0xce: 12, 0xcf: 13}
    expected, padded = {}, {}
    for frame, data in zip(frames, dumps, strict=True):
        p, w, h, count = struct.unpack_from("<4I", data, 4)
        assert (p, w, h, count) == (frame["precision"], frame["width"], frame["height"], len(frame["components"]))
        kind = "L" if data[:4] == b"JLP1" else "D"
        position = 20
        for _ in range(count):
            if kind == "L":
                component_id, _, _, cw, ch = struct.unpack_from("<5I", data, position)
                position += 20
                length = cw * ch
                values = struct.unpack_from("<" + "H" * length, data, position)
                position += 2 * length
            else:
                component_id, _, _, _, _, bw, bh = struct.unpack_from("<7I", data, position)
                position += 28 + 128  # original quantizer snapshot
                length = bw * bh * 64
                values = struct.unpack_from("<" + "h" * length, data, position)
                position += 2 * length
            key = (kind, scan_types[frame["marker"]], w, h, component_id)
            assert key not in expected
            expected[key] = list(values)
            padded[key] = cw * ((ch + 7) // 8 * 8) if kind == "L" else length
        assert position == len(data)
    actual = {}
    for line in observations.splitlines():
        fields = line.split()
        key = (fields[0], *(int(v) for v in fields[1:5]))
        values = [int(v) for v in fields[5:]]
        if key[0] == "L":
            values = [v & 65535 for v in values]
        actual.setdefault(key, []).extend(values)
    assert actual.keys() == expected.keys(), (actual.keys(), expected.keys())
    for key, values in expected.items():
        assert len(values) <= len(actual[key]) <= padded[key], (key, len(values), len(actual[key]))
        assert actual[key][:len(values)] == values, (key, len(values), len(actual[key]),
                                      [(i, a, b) for i, (a, b) in enumerate(zip(values, actual[key])) if a != b][:10])
    return sum(len(v) // 64 for k, v in expected.items() if k[0] == "D"), sum(len(v) for k, v in expected.items() if k[0] == "L")

def lossless_reconstruction(header, frames, dumps):
    """Independent integer Annex J composition from C-checked raw frame rows."""
    planes = {}
    for frame, data in zip(frames, dumps, strict=True):
        _, _, _, components = samples(data)
        for shape, values in components:
            component_id, _, _, width, height = shape
            if frame["marker"] in (0xc3, 0xcb):
                result = list(values)
            else:
                rows = planes[component_id]
                if frame["expansion"] & 0x10:
                    rows = [[v for pair in zip(row, row[1:] + row[-1:])
                             for v in (pair[0], (pair[0] + pair[1]) // 2)] for row in rows]
                if frame["expansion"] & 1:
                    rows = [row for first, second in zip(rows, rows[1:] + rows[-1:])
                            for row in (first, [(a + b) // 2 for a, b in zip(first, second, strict=True)])]
                reference = [v for row in rows[:height] for v in row[:width]]
                assert len(reference) == len(values)
                result = [(a + b) & 65535 for a, b in zip(reference, values, strict=True)]
            planes[component_id] = [result[y * width:(y + 1) * width] for y in range(height)]
    return [tuple(v for row in planes[component_id] for v in row) for component_id, _, _ in header["components"]]
