#!/usr/bin/env python3
"""Generate the Android launcher icons from `assets/icon.png`.

Upstream's `generate_icon.py` produces the *window* icon — raw RGBA for macroquad's `Conf.icon`,
written to `phira/icon/`. Android needs something else: density-bucketed PNGs under
`res/mipmap-*`. This script writes those, and lives beside that one rather than inside it.

Usage:
    python scripts/android_icon.py [source.png] [res-dir]
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageOps

#: 48 dp is the baseline; the rest are the standard multipliers.
DENSITIES = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    src = Path(sys.argv[1]) if len(sys.argv) > 1 else root / "assets" / "icon.png"
    res = Path(sys.argv[2]) if len(sys.argv) > 2 else root / "android" / "app" / "src" / "main" / "res"

    if not src.is_file():
        raise SystemExit(f"icon not found: {src}")

    img = Image.open(src).convert("RGBA")
    resample = getattr(Image, "Resampling", Image).LANCZOS

    for density, size in DENSITIES.items():
        out = res / f"mipmap-{density}" / "ic_launcher.png"
        out.parent.mkdir(parents=True, exist_ok=True)
        ImageOps.fit(img, (size, size), method=resample).save(out, "PNG", optimize=True)
        print(f"{size:>3}x{size:<3} -> {out.relative_to(root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
