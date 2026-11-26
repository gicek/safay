#!/usr/bin/env python3
"""Derive the phone and tablet icons from assets/icon.png.

The app icon is drawn for macOS: artwork on a dark rounded square, inset on a transparent canvas. Phones cut their own
shape from an opaque square, so this redraws the icon edge to edge: the square's gradient across the whole canvas, with
the artwork on it at the size it has relative to the rounded square.

- Android launchers cut their shape from a 108 dp canvas and show its middle 72 dp: the foreground layer is the gradient
  and the artwork for that canvas. It is opaque, so the launch screen, which shows the foreground alone cut to a circle,
  fills that circle too; the background layer, a color, never shows. The monochrome layer is the artwork's silhouette,
  tinted by the system for themed icons.
- iOS masks the whole 1024 px square itself: the gradient and the artwork fill it, without transparency.

Run it again after assets/icon.png changes; it writes assets/android/ and assets/ios/.
"""

import math
import random
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets/icon.png"
OUT = ROOT / "assets/android"
IOS = ROOT / "assets/ios"
SIZE = 1024

# Measured on assets/icon.png: the rounded square spans 101..923 on both axes, filled with a vertical gray gradient
# from 50 at its top to 19 at its bottom. The artwork lies within 340 px of the center; the check badge is centered at
# (636.5, 657.5), 70 px across its colored disc.
SQUARE_TOP, SQUARE_BOTTOM = 101, 923
GRAY_TOP, GRAY_BOTTOM = 50.0, 19.0
ART_RADIUS = 340
BADGE_CENTER, BADGE_RADIUS = (636.5, 657.5), 70

# Android: the visible 72 dp of the 108 dp canvas takes the place of the rounded square. iOS: the whole canvas does.
ANDROID = (SIZE * 72 / 108) / (SQUARE_BOTTOM - SQUARE_TOP)
WHOLE = SIZE / (SQUARE_BOTTOM - SQUARE_TOP)


def gray(y: float) -> float:
    """The square's gradient at row `y` of the source, extended past its edges."""
    return GRAY_TOP + (y - SQUARE_TOP) * (GRAY_BOTTOM - GRAY_TOP) / (SQUARE_BOTTOM - SQUARE_TOP)


def to_source(v: float, scale: float) -> float:
    """A coordinate of an output canvas drawn at `scale`, in the source."""
    return SIZE / 2 + (v - SIZE / 2) / scale


def art_mask() -> Image.Image:
    """The disc around the artwork, feathered so its edge melts into the gradient around it."""
    mask = Image.new("L", (SIZE, SIZE), 0)
    c = SIZE / 2
    ImageDraw.Draw(mask).ellipse((c - ART_RADIUS, c - ART_RADIUS, c + ART_RADIUS, c + ART_RADIUS), fill=255)
    return mask.filter(ImageFilter.GaussianBlur(12))


def place(image: Image.Image, scale: float) -> Image.Image:
    """A source-sized layer, scaled about the center onto the output canvas (cropped where it is larger)."""
    side = round(SIZE * scale)
    scaled = image.resize((side, side), Image.LANCZOS)
    canvas = Image.new(image.mode, (SIZE, SIZE), 0)
    offset = (SIZE - side) // 2
    canvas.paste(scaled, (offset, offset))
    return canvas if side <= SIZE else scaled.crop((-offset, -offset, -offset + SIZE, -offset + SIZE))


def gradient(scale: float) -> Image.Image:
    # Dithered by a fraction of a level, so the slow gradient shows no bands.
    rng = random.Random(0)
    image = Image.new("RGB", (SIZE, SIZE))
    pixels = image.load()
    for y in range(SIZE):
        level = gray(to_source(y + 0.5, scale))
        for x in range(SIZE):
            v = max(0, min(255, round(level + rng.uniform(-0.5, 0.5))))
            pixels[x, y] = (v, v, v)
    return image


def foreground(source: Image.Image, scale: float) -> Image.Image:
    """The gradient edge to edge with the artwork on it, drawn at `scale`; opaque."""
    art = source.convert("RGB").convert("RGBA")
    art.putalpha(art_mask())
    return Image.alpha_composite(gradient(scale).convert("RGBA"), place(art, scale)).convert("RGB")


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
    return place(layer, ANDROID)


def main() -> None:
    source = Image.open(SOURCE)
    if source.size != (SIZE, SIZE):
        raise SystemExit(f"{SOURCE} is {source.size[0]}x{source.size[1]}, expected {SIZE}x{SIZE}")
    OUT.mkdir(exist_ok=True)
    foreground(source, ANDROID).save(OUT / "foreground.png", optimize=True)
    monochrome(source).save(OUT / "monochrome.png", optimize=True)
    IOS.mkdir(exist_ok=True)
    foreground(source, WHOLE).save(IOS / "icon.png", optimize=True)


if __name__ == "__main__":
    main()
