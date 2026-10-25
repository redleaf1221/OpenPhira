#!/usr/bin/env python3
"""Turn packaged resources back into the plaintext the repository stores.

The inverse of `encrypt_assets.py`: reads `custom_enc(zstd(payload))` and writes `payload`.
Point it at a directory or at a retail APK — either way it writes the same tree the engine
expects to find under `assets/res/`.

This is how the repository's own `phira/assets/res/**` is kept: plaintext, no cipher, no zstd
wrapper. Encryption happens once, at packaging time.

Usage:
    python decrypt_assets.py <apk | assets-dir> <out-dir>
"""

from __future__ import annotations

import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from encrypt_assets import needs_encryption, unpack  # noqa: E402


def from_apk(apk: Path, out: Path) -> int:
    n = 0
    with zipfile.ZipFile(apk) as zf:
        for name in sorted(zf.namelist()):
            if not name.startswith("assets/"):
                continue
            rel = Path(name[len("assets/"):])
            if not needs_encryption(rel):
                continue
            dst = out / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(unpack(zf.read(name)))
            print(f"  {rel}")
            n += 1
    return n


def from_dir(src: Path, out: Path) -> int:
    n = 0
    for path in sorted(src.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(src)
        if not needs_encryption(rel):
            continue
        dst = out / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(unpack(path.read_bytes()))
        print(f"  {rel}")
        n += 1
    return n


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)

    n = from_apk(src, out) if src.is_file() else from_dir(src, out)
    print(f"{n} resource(s) -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
