#!/usr/bin/env python3
"""Check that a built APK carries the repository's resources, correctly repacked.

Every `res/**` file in the APK must decrypt and inflate back to the plaintext in the repository.
That covers both layers of the packaging step — the cipher and the zstd wrapper — against a
known-good input, rather than checking that the packer agrees with itself.

Usage:
    python verify_package.py <apk> <assets-dir>
"""

from __future__ import annotations

import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from encrypt_assets import needs_encryption, unpack  # noqa: E402


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    apk, assets = Path(sys.argv[1]), Path(sys.argv[2])

    ok = fail = 0
    with zipfile.ZipFile(apk) as zf:
        for name in sorted(zf.namelist()):
            if not name.startswith("assets/"):
                continue
            rel = Path(name[len("assets/"):])
            if not needs_encryption(rel):
                continue

            expected = assets / rel
            if not expected.is_file():
                print(f"FAIL  {rel}  not present in {assets}")
                fail += 1
                continue

            if unpack(zf.read(name)) != expected.read_bytes():
                print(f"FAIL  {rel}  payload differs")
                fail += 1
                continue

            print(f"OK    {rel}")
            ok += 1

    print(f"\n{ok} OK, {fail} FAIL")
    return 0 if fail == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
