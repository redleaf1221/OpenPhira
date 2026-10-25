#!/usr/bin/env python3
"""Encrypt the built-in resources on their way into the APK.

On disk the engine expects `custom_enc(zstd(payload))`, because `phira::inner::resolve_data` is
`zstd::decode_all(custom_dec(bytes))`.  The repository keeps the plaintext payload, so this
script does both layers: compress, then encrypt.

Files under `res/` that end in `.yml` are metadata the engine reads as text and are copied
untouched.

The cipher is a byte-exact port of `phira/src/inner.rs::custom_enc`; `verify_cipher.py` checks it
against a retail package.

Usage:
    python encrypt_assets.py <src-assets-dir> <dst-assets-dir> [--verify]
"""

from __future__ import annotations

import argparse
import shutil
import sys
from compression import zstd
from pathlib import Path

BLOCK = 8

STAGE_1 = [(1, 0), (3, 2), (5, 4), (7, 6)]
STAGE_2 = [(2, 0), (3, 1), (6, 4), (7, 5)]
STAGE_4 = [(4, 0), (5, 1), (6, 2), (7, 3)]

#: Kept in the clear: the engine parses these as YAML before it ever decrypts anything.
PLAINTEXT_SUFFIXES = {".yml", ".yaml"}

#: Any zstd frame decodes identically, so the level is only a size/speed knob.
ZSTD_LEVEL = 9


def custom_enc(data: bytes) -> bytes:
    """Pad, then apply the inverse Walsh-Hadamard butterfly. Mirrors `inner::custom_enc`."""
    pad = (-len(data)) % BLOCK
    if pad == 0:
        pad = BLOCK
    marker = BLOCK - pad

    b = bytearray(data) + bytes([marker]) * pad
    for i in range(0, len(b), BLOCK):
        b[i + 3], b[i + 5] = b[i + 5], b[i + 3]
        for dst, src in reversed(STAGE_4):
            b[i + dst] = (b[i + dst] + b[i + src]) & 0xFF
        for dst, src in reversed(STAGE_2):
            b[i + dst] = (b[i + dst] + b[i + src]) & 0xFF
        for dst, src in reversed(STAGE_1):
            b[i + dst] = (b[i + dst] + b[i + src]) & 0xFF
        b[i + 3], b[i + 6] = b[i + 6], b[i + 3]
        b[i + 2], b[i + 6] = b[i + 6], b[i + 2]
    return bytes(b)


def custom_dec(data: bytes) -> bytes:
    """Inverse of [`custom_enc`], including the length recovery. Mirrors `inner::custom_dec`."""
    b = bytearray(data)
    end = len(b) - len(b) % BLOCK
    for i in range(0, end, BLOCK):
        b[i + 2], b[i + 6] = b[i + 6], b[i + 2]
        b[i + 3], b[i + 6] = b[i + 6], b[i + 3]
        for dst, src in STAGE_1:
            b[i + dst] = (b[i + dst] - b[i + src]) & 0xFF
        for dst, src in STAGE_2:
            b[i + dst] = (b[i + dst] - b[i + src]) & 0xFF
        for dst, src in STAGE_4:
            b[i + dst] = (b[i + dst] - b[i + src]) & 0xFF
        b[i + 3], b[i + 5] = b[i + 5], b[i + 3]

    if b:
        plain_len = len(b) + b[-1] - BLOCK
        if 0 <= plain_len <= len(b):
            del b[plain_len:]
    return bytes(b)


def pack(payload: bytes) -> bytes:
    """Plaintext resource -> what the APK ships."""
    return custom_enc(zstd.compress(payload, level=ZSTD_LEVEL))


def unpack(blob: bytes) -> bytes:
    """What the APK ships -> plaintext resource."""
    return zstd.decompress(custom_dec(blob))


def needs_encryption(rel: Path) -> bool:
    """Only `res/**` is protected, and YAML stays readable."""
    parts = rel.parts
    if not parts or parts[0] != "res":
        return False
    return rel.suffix.lower() not in PLAINTEXT_SUFFIXES


def encrypt_tree(src: Path, dst: Path, verify: bool) -> tuple[int, int]:
    encrypted = copied = 0
    for path in sorted(src.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(src)
        out = dst / rel
        out.parent.mkdir(parents=True, exist_ok=True)

        payload = path.read_bytes()
        if needs_encryption(rel):
            blob = pack(payload)
            if verify and unpack(blob) != payload:
                raise SystemExit(f"round-trip mismatch: {rel}")
            out.write_bytes(blob)
            encrypted += 1
        else:
            shutil.copyfile(path, out)
            copied += 1
    return encrypted, copied


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("src", type=Path)
    ap.add_argument("dst", type=Path)
    ap.add_argument("--verify", action="store_true", help="decrypt every blob again before writing")
    args = ap.parse_args()

    if not args.src.is_dir():
        raise SystemExit(f"not a directory: {args.src}")

    enc, plain = encrypt_tree(args.src, args.dst, args.verify)
    print(f"{enc} encrypted, {plain} copied verbatim -> {args.dst}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
