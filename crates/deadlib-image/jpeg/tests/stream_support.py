"""Independent test-only marker ranges and raw DQT/DHT destination storage."""


def marker_ranges(data):
    assert data[:2] == b"\xff\xd8"
    position = 2
    while position < len(data):
        start = position
        assert data[position] == 255
        while data[position] == 255:
            position += 1
        marker = data[position]
        position += 1
        if marker == 0xd9:
            yield marker, start, position
            return
        size = int.from_bytes(data[position:position + 2], "big")
        assert size >= 2
        end = position + size
        if marker == 0xda:
            while end < len(data):
                if data[end] != 255:
                    end += 1
                else:
                    j = end + 1
                    while data[j] == 255:
                        j += 1
                    if data[j] == 0 or 0xd0 <= data[j] <= 0xd7:
                        end = j + 1
                    else:
                        break
        yield marker, start, end
        position = end
    raise AssertionError("missing EOI")


def update_tables(bank, data, table_stream=False):
    for marker, start, end in marker_ranges(data):
        if table_stream:
            assert marker in (0xdb, 0xc4, 0xd9, 0xfe) or 0xe0 <= marker <= 0xef
        if marker not in (0xdb, 0xc4):
            continue
        position = start + 4
        while position < end:
            selector = data[position]
            size = 1 + 64 * ((selector >> 4) + 1) if marker == 0xdb else 17 + sum(data[position + 1:position + 17])
            assert position + size <= end and selector & 15 <= 3
            bank[marker, selector & 15 if marker == 0xdb else selector] = data[position:position + size]
            position += size
        assert position == end


def prefix_tables(bank, image):
    """Test-only equivalent of external table application; entropy unchanged."""
    out = bytearray(image[:2])
    for (marker, _), payload in sorted(bank.items()):
        out.extend(bytes([255, marker]) + (len(payload) + 2).to_bytes(2, "big") + payload)
    out.extend(image[2:])
    return bytes(out)


def without(data, markers):
    out = bytearray(data[:2])
    for marker, start, end in marker_ranges(data):
        if marker not in markers:
            out.extend(data[start:end])
    return bytes(out)


def clear_stale_exp(data, frames):
    out, fi, previous = bytearray(data[:2]), 0, 0
    differential = (0xc5, 0xc6, 0xc7, 0xcd, 0xce, 0xcf)
    for marker, start, end in marker_ranges(data):
        if marker in (0xc0, 0xc1, 0xc2, 0xc3, 0xc9, 0xca, 0xcb) + differential:
            frame = frames[fi]
            fi += 1
            if marker in differential and previous and not frame["expansion"]:
                out.extend(b"\xff\xdf\x00\x03\x00")
            previous = frame["expansion"]
        out.extend(data[start:end])
    return bytes(out)
