"""Bake the supplied static pad geometry; this is an asset-authoring tool only."""
from pathlib import Path
import shlex
import struct
import sys


def convert(source: Path, destination: Path) -> None:
    lines = [line.strip() for line in source.read_text(encoding="utf-8-sig").splitlines()
             if line.strip() and not line.lstrip().startswith("//")]
    cursor = next(i for i, line in enumerate(lines) if line.startswith("Meshes:"))
    mesh_count = int(lines[cursor].split(":")[1])
    cursor += 1
    groups = {}
    for _ in range(mesh_count):
        material = int(shlex.split(lines[cursor])[2])
        cursor += 1
        count = int(lines[cursor]); cursor += 1
        vertices = [list(map(float, line.split())) for line in lines[cursor:cursor + count]]
        cursor += count
        count = int(lines[cursor]); cursor += 1
        normals = [list(map(float, line.split())) for line in lines[cursor:cursor + count]]
        cursor += count
        count = int(lines[cursor]); cursor += 1
        for line in lines[cursor:cursor + count]:
            triangle = list(map(int, line.split()))
            for vertex_index, normal_index in zip(triangle[1:4], triangle[4:7]):
                vertex = vertices[vertex_index]
                groups.setdefault(material, []).append((*vertex[1:4], *normals[normal_index], *vertex[4:6]))
        cursor += count
    assert lines[cursor].startswith("Materials:")
    count = int(lines[cursor].split(":")[1]); cursor += 1
    output = bytearray(b"PAD1" + struct.pack("<I", count))
    textures = []
    for material in range(count):
        name = lines[cursor]; cursor += 1
        ambient = list(map(float, lines[cursor].split())); cursor += 1
        diffuse = list(map(float, lines[cursor].split())); cursor += 1
        cursor += 4  # specular, emissive, shininess, transparency
        textures.append(shlex.split(lines[cursor])[0] if lines[cursor] != '""' else "")
        cursor += 2
        vertices = groups.get(material, [])
        output.extend(struct.pack("<8fI", *ambient, *diffuse, len(vertices)))
        for vertex in vertices:
            output.extend(struct.pack("<8f", *vertex))
        print(name, len(vertices), "vertices")
    assert textures == ["MetalPanel.png", "Arrow-Blue.png", "Arrow-Red.png", ""], textures
    assert lines[cursor] == "Bones: 0", "The supplied training pad must be static."
    destination.write_bytes(output)


if __name__ == "__main__":
    convert(Path(sys.argv[1]), Path(__file__).parent.parent / "assets" / "training-pad.bin")
