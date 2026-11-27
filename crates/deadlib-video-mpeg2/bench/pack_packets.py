"""Store container dimensions and padded picture packets for both decoders."""

import json
import struct
import subprocess
import sys
from pathlib import Path


def decoder_config(source):
    result = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-select_streams", "v:0",
        "-show_entries", "stream=extradata", "-show_data", "-of", "json", str(source),
    ]))
    stream, = result["streams"]
    return b"".join(
        bytes.fromhex(line.partition(":")[2].split("  ", 1)[0])
        for line in stream["extradata"].splitlines() if ":" in line
    )


def main():
    codec, source, destination = sys.argv[1:]
    source, destination = Path(source), Path(destination)
    probe = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-select_streams", "v:0",
        "-show_entries", "stream=codec_name,width,height:packet=pos,size",
        "-of", "json", str(source),
    ]))
    stream, = probe["streams"]
    assert stream["codec_name"] == codec, source
    with source.open("rb") as container, destination.open("wb") as packets:
        packets.write(struct.pack("<II", stream["width"], stream["height"]))
        if codec == "hevc":
            config = decoder_config(source)
            assert len(config) >= 23 and config[0] == 1, source
            packets.write(struct.pack("<I", len(config)))
            packets.write(config)
        for packet in probe["packets"]:
            size = int(packet["size"])
            if size == 0:
                continue
            container.seek(int(packet["pos"]))
            payload = container.read(size)
            assert len(payload) == size, source
            packets.write(struct.pack("<I", size))
            packets.write(payload)
            packets.write(bytes(64))  # AV_INPUT_BUFFER_PADDING_SIZE


if __name__ == "__main__":
    main()
