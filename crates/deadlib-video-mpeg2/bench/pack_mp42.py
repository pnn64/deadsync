"""Store AVI dimensions and padded picture packets for both decoders."""

import json
import struct
import subprocess
import sys
from pathlib import Path


def main():
    source, destination = map(Path, sys.argv[1:])
    probe = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-select_streams", "v:0",
        "-show_entries", "stream=codec_name,width,height:packet=pos,size",
        "-of", "json", str(source),
    ]))
    stream, = probe["streams"]
    assert stream["codec_name"] == "msmpeg4v2", source
    with source.open("rb") as avi, destination.open("wb") as packets:
        packets.write(struct.pack("<II", stream["width"], stream["height"]))
        for packet in probe["packets"]:
            size = int(packet["size"])
            if size == 0:
                continue
            avi.seek(int(packet["pos"]))
            payload = avi.read(size)
            assert len(payload) == size, source
            packets.write(struct.pack("<I", size))
            packets.write(payload)
            packets.write(bytes(64))  # AV_INPUT_BUFFER_PADDING_SIZE


if __name__ == "__main__":
    main()
