#!/usr/bin/env python3
"""Copies the README's screenshots from docs/screenshots into docs/img, with clean window corners.

The macOS captures (previews/screenshots.test.tsx) leave a faint shadow under the window's bottom corners, and where NSH
draws the window in process, square top corners over an opaque fill. Each picture gets the window's own corner shape,
read from its bottom left corner, at all four corners, with nothing but transparency outside it, so it sits on any page
background.

Needs Pillow (`python3 -m pip install Pillow`).
"""

import sys
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    sys.exit("readme_images.py needs Pillow: python3 -m pip install Pillow")

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "docs" / "screenshots"
TARGET = ROOT / "docs" / "img"
# What the README shows.
NAMES = ["accounts-light", "accounts-dark", "detail", "backup-locations", "settings-general", "panel-light"]
# Large enough for a macOS window's corner at 2x.
CORNER = 96
# Alpha at or below this is the shadow outside the window; at full opacity it is the window.
SHADOW = 60


def corner_mask(alpha: Image.Image) -> Image.Image:
    """The window's coverage in its bottom left corner (CORNER × CORNER), from transparent to opaque."""
    w, h = alpha.size
    block = alpha.crop((0, h - CORNER, CORNER, h))
    return block.point(lambda a: max(0, min(255, round((a - SHADOW) * 255 / (255 - SHADOW)))))


def round_corners(image: Image.Image) -> Image.Image:
    image = image.convert("RGBA")
    w, h = image.size
    alpha = image.getchannel("A")
    bottom_left = corner_mask(alpha)
    masks = {
        (0, h - CORNER): bottom_left,
        (w - CORNER, h - CORNER): bottom_left.transpose(Image.Transpose.FLIP_LEFT_RIGHT),
        (0, 0): bottom_left.transpose(Image.Transpose.FLIP_TOP_BOTTOM),
        (w - CORNER, 0): bottom_left.transpose(Image.Transpose.ROTATE_180),
    }
    for (x, y), mask in masks.items():
        alpha.paste(mask, (x, y))
    image.putalpha(alpha)
    return image


def main() -> int:
    TARGET.mkdir(parents=True, exist_ok=True)
    missing = [n for n in NAMES if not (SOURCE / f"{n}.png").exists()]
    if missing:
        print(f"missing in {SOURCE.relative_to(ROOT)}: {', '.join(missing)}; take the screenshots first", file=sys.stderr)
        return 1
    for name in NAMES:
        with Image.open(SOURCE / f"{name}.png") as image:
            round_corners(image).save(TARGET / f"{name}.png", optimize=True)
        print(f"docs/img/{name}.png")
    return 0


if __name__ == "__main__":
    sys.exit(main())
