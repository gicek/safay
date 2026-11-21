#!/usr/bin/env python3
"""Derive the Android adaptive launcher icon from assets/icon.png.

The app icon is drawn for macOS: artwork on a dark rounded square, inset on a transparent canvas. Android launchers cut
their own shape from a 108 dp canvas and show its middle 72 dp, so this redraws the icon for that canvas: the foreground
layer is the square's gradient edge to edge with the artwork on it, at the same size relative to the visible 72 dp as it
has to the rounded square. It is opaque, so the launch screen, which shows the foreground alone cut to a circle, fills
that circle too; the background layer, a color, never shows. The monochrome layer is the artwork's silhouette, tinted by
the system for themed icons.

Run it again after assets/icon.png changes; it writes assets/android/.
"""

import math
import random
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets/icon.png"
OUT = ROOT / "assets/android"
SIZE = 1024

# Measured on assets/icon.png: the rounded square spans 101..923 on both axes, filled with a vertical gray gradient
# from 50 at its top to 19 at its bottom. The artwork lies within 340 px of the center; the check badge is centered at
# (636.5, 657.5), 70 px across its colored disc.
SQUARE_TOP, SQUARE_BOTTOM = 101, 923
GRAY_TOP, GRAY_BOTTOM = 50.0, 19.0
ART_RADIUS = 340
BADGE_CENTER, BADGE_RADIUS = (636.5, 657.5), 70

# The visible 72 dp of the 108 dp canvas takes the place of the rounded square.
SCALE = (SIZE * 72 / 108) / (SQUARE_BOTTOM - SQUARE_TOP)


def gray(y: float) -> float:
    """The square's gradient at row `y` of the source, extended past its edges."""
    return GRAY_TOP + (y - SQUARE_TOP) * (GRAY_BOTTOM - GRAY_TOP) / (SQUARE_BOTTOM - SQUARE_TOP)


def to_source(v: float) -> float:
    """A coordinate of the output canvas, in the source."""
    return SIZE / 2 + (v - SIZE / 2) / SCALE


def art_mask() -> Image.Image:
    """The disc around the artwork, feathered so its edge melts into the gradient around it."""
    mask = Image.new("L", (SIZE, SIZE), 0)
    c = SIZE / 2
    ImageDraw.Draw(mask).ellipse((c - ART_RADIUS, c - ART_RADIUS, c + ART_RADIUS, c + ART_RADIUS), fill=255)
    return mask.filter(ImageFilter.GaussianBlur(12))


def place(image: Image.Image) -> Image.Image:
    """A source-sized layer, scaled about the center onto the output canvas."""
    side = round(SIZE * SCALE)
    scaled = image.resize((side, side), Image.LANCZOS)
    canvas = Image.new(image.mode, (SIZE, SIZE), 0)
    offset = (SIZE - side) // 2
    canvas.paste(scaled, (offset, offset))
    return canvas


def gradient() -> Image.Image:
    # Dithered by a fraction of a level, so the slow gradient shows no bands.
    rng = random.Random(0)
    image = Image.new("RGB", (SIZE, SIZE))
    pixels = image.load()
    for y in range(SIZE):
        level = gray(to_source(y + 0.5))
        for x in range(SIZE):
            v = max(0, min(255, round(level + rng.uniform(-0.5, 0.5))))
            pixels[x, y] = (v, v, v)
    return image


def foreground(source: Image.Image) -> Image.Image:
    art = source.convert("RGB").convert("RGBA")
    art.putalpha(art_mask())
    return Image.alpha_composite(gradient().convert("RGBA"), place(art)).convert("RGB")


def monochrome(source: Image.Image) -> Image.Image:
    """White where the artwork stands out from the gradient by brightness; the keyhole, the ring around the badge and
    the check stay open."""
    rgb = source.convert("RGB")
    luma = rgb.convert("L").load()
    channels = rgb.load()
    alpha = Image.new("L", (SIZE, SIZE), 0)
    out = alpha.load()
    c = SIZE / 2
    for y in range(SIZE):
        b = gray(y)
        for x in range(SIZE):
            if math.hypot(x - c, y - c) > ART_RADIUS:
                continue
            a = max(0.0, min(1.0, (abs(luma[x, y] - b) - 12) / 12))
            if math.hypot(x - BADGE_CENTER[0], y - BADGE_CENTER[1]) < BADGE_RADIUS:
                white = max(0.0, min(1.0, (min(channels[x, y]) - 150) / 60))
                a *= 1 - white
            out[x, y] = round(a * 255)
    layer = Image.new("RGBA", (SIZE, SIZE), (255, 255, 255, 0))
    layer.putalpha(ImageChops.multiply(alpha, art_mask()))
    return place(layer)


def main() -> None:
    source = Image.open(SOURCE)
    if source.size != (SIZE, SIZE):
        raise SystemExit(f"{SOURCE} is {source.size[0]}x{source.size[1]}, expected {SIZE}x{SIZE}")
    OUT.mkdir(exist_ok=True)
    foreground(source).save(OUT / "foreground.png", optimize=True)
    monochrome(source).save(OUT / "monochrome.png", optimize=True)


if __name__ == "__main__":
    main()
