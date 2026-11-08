"""Portable hashes for capture receipts; Git may check text out as CRLF."""
import hashlib


TEXT_SUFFIXES = {".json", ".lua", ".sm", ".ssc", ".ini", ".txt", ".xml",
                 ".vert", ".frag", ".glsl", ".patch", ".rs", ".py"}
HASH_ENCODING = "sha256-lf-text-v1"


def digest(path):
    raw = path.read_bytes()
    if path.suffix.lower() in TEXT_SUFFIXES:
        raw = raw.replace(b"\r\n", b"\n")
    return hashlib.sha256(raw).hexdigest()
