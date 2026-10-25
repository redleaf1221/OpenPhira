#!/usr/bin/env python3
"""Check the cipher implementation against a real APK.

The packaged `assets/res/**` files are `custom_enc(zstd(payload))`.  If our `custom_enc` is
byte-exact, decrypting and re-encrypting an asset must reproduce the file bit for bit — that is
an external check, unlike a round-trip test, which only proves the two halves agree with each
other.

Also asserts that every decrypted blob starts with the zstd frame magic, i.e. that the outer
cipher is gone.

Usage:
    python verify_cipher.py [path-to.apk]
"""

from __future__ import annotations

import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from encrypt_assets import BLOCK, custom_dec, custom_enc  # noqa: E402

ZSTD_MAGIC = b"\x28\xb5\x2f\xfd"


def main() -> int:
    apk = Path(sys.argv[1] if len(sys.argv) > 1 else "Phira.apk")
    ok = fail = skip = 0

    with zipfile.ZipFile(apk) as zf:
        names = sorted(n for n in zf.namelist() if n.startswith("assets/res/"))
        for name in names:
            raw = zf.read(name)
            if len(raw) % BLOCK:
                print(f"SKIP  {name:44s} {len(raw):9d}  not block aligned (plaintext)")
                skip += 1
                continue

            framed = custom_dec(raw)
            if framed[:4] != ZSTD_MAGIC:
                print(f"FAIL  {name:44s} {len(raw):9d}  missing zstd magic")
                fail += 1
                continue

            if custom_enc(framed) != raw:
                print(f"FAIL  {name:44s} {len(raw):9d}  re-encrypt differs")
                fail += 1
                continue

            print(f"OK    {name:44s} {len(raw):9d}")
            ok += 1

    print(f"\n{ok} OK, {fail} FAIL, {skip} skipped")
    return 0 if fail == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
