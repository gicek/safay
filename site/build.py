#!/usr/bin/env python3
"""Builds the Safay website (safay.gicek.com) into build/site/dist/.

Usage (from the repository root, through make):
  python3 site/build.py            # render pages for every language and copy assets
  python3 site/build.py --shots    # turn docs/screenshots/ into site/shots/*.webp and the logo sizes (needs Pillow)

Pages are plain HTML templates in site/src/. Placeholders:
  {{key}}            string from site/i18n/<lang>.json (a missing key fails the build)
  {{@lang}}          BCP-47 code of the page (en, zh-Hans, zh-Hant)
  {{@root}}          URL prefix of the language ("/", "/zh-hans/", "/zh-hant/")
  {{@page}}          page path below the root ("", "privacy/")
  {{@version}}       the workspace version from Cargo.toml
  {{@title}}         the page's title and description, from the keys named in PAGES
  {{@description}}
  {{@canonical}}     absolute URL of the page; {{@og_locale}} its Open Graph locale; {{@og_image}} its preview image
  {{@shot NAME}}     /assets/shots/NAME-<lang>.webp, falling back to English when that language has none
  {{@alternates}}    <link rel="alternate" hreflang> tags for the page
  {{@languages}}     language switcher links
  {{@include NAME}}  the partial site/src/_NAME.html, rendered with the same placeholders
  {{@link NAME}}     a link from site/links.toml ({version} filled in)
  {{@first KEY}}     the first entry of a "|"-separated string (the hero's rotating word before the script runs)
Needs Python 3.11 or later (tomllib); building has no dependencies beyond the standard library.
"""

from __future__ import annotations

import json
import re
import shutil
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
DIST = ROOT / "build" / "site" / "dist"
LINKS = tomllib.loads((SITE / "links.toml").read_text())
SITE_URL = LINKS["site"]

LANGUAGES = [
    # code, URL prefix, name shown in the switcher, Open Graph locale
    ("en", "/", "English", "en_US"),
    ("zh-Hans", "/zh-hans/", "简体中文", "zh_CN"),  # i18n-guard: allow language self-name
    ("zh-Hant", "/zh-hant/", "繁體中文", "zh_TW"),  # i18n-guard: allow language self-name
]
# template file -> page path below the language root, title key, description key
PAGES = {
    "index.html": ("", "meta_title", "meta_description"),
    "privacy.html": ("privacy/", "privacy_meta_title", "privacy_meta_description"),
}

PLACEHOLDER = re.compile(r"\{\{\s*(@?[\w.]+)(?:\s+([\w-]+))?\s*\}\}")


def version() -> str:
    package = tomllib.loads((ROOT / "Cargo.toml").read_text()).get("workspace", {}).get("package", {})
    if "version" not in package:
        sys.exit("error: [workspace.package] version not found in Cargo.toml")
    return package["version"]


def load_strings() -> dict[str, dict[str, str]]:
    strings = {code: json.loads((SITE / "i18n" / f"{code}.json").read_text()) for code, *_ in LANGUAGES}
    reference = set(strings["en"])
    for code, table in strings.items():
        missing, extra = reference - set(table), set(table) - reference
        if missing or extra:
            sys.exit(f"error: site/i18n/{code}.json differs from en.json: missing {sorted(missing)}, extra {sorted(extra)}")
    return strings


def shot_path(name: str, lang: str) -> str:
    for candidate in (lang, "en"):
        if (SITE / "shots" / f"{name}-{candidate}.webp").exists():
            return f"/assets/shots/{name}-{candidate}.webp"
    sys.exit(f"error: no screenshot site/shots/{name}-en.webp (run make site-shots)")


def links(ver: str) -> dict[str, str]:
    return {name: url.format(version=ver) for name, url in LINKS.items() if isinstance(url, str)}


def render(template: str, lang: str, root: str, page: str, strings: dict[str, str], ver: str, meta: tuple[str, str] = ("", "")) -> str:
    title_key, description_key = meta
    locale = next(og for code, _, _, og in LANGUAGES if code == lang)

    def alternates() -> str:
        tags = [f'<link rel="alternate" hreflang="{code}" href="{SITE_URL}{prefix}{page}">' for code, prefix, *_ in LANGUAGES]
        tags.append(f'<link rel="alternate" hreflang="x-default" href="{SITE_URL}/{page}">')
        return "\n    ".join(tags)

    def languages() -> str:
        items = []
        for code, prefix, name, _ in LANGUAGES:
            current = ' aria-current="true"' if code == lang else ""
            items.append(f'<a href="{prefix}{page}" hreflang="{code}" lang="{code}" data-lang="{code}"{current}>{name}</a>')
        return "".join(items)

    def substitute(match: re.Match[str]) -> str:
        key, argument = match.group(1), match.group(2)
        if key == "@lang":
            return lang
        if key == "@root":
            return root
        if key == "@page":
            return page
        if key == "@version":
            return ver
        if key in ("@title", "@description"):
            string_key = title_key if key == "@title" else description_key
            if not string_key:
                sys.exit(f"error: {key} is not available on this page")
            return strings[string_key]
        if key == "@canonical":
            return f"{SITE_URL}{root}{page}"
        if key == "@og_locale":
            return locale
        if key == "@og_image":
            return f"{SITE_URL}{shot_path('main-split', lang)}"
        if key == "@shot":
            return shot_path(argument or "", lang)
        if key == "@alternates":
            return alternates()
        if key == "@languages":
            return languages()
        if key == "@include":
            return (SITE / "src" / f"_{argument}.html").read_text()
        if key == "@link":
            table = links(ver)
            if argument not in table:
                sys.exit(f"error: unknown link {argument!r}")
            return table[argument]
        if key == "@first":
            return strings[argument or ""].split("|")[0]
        if key not in strings:
            sys.exit(f"error: missing string {key!r}")
        # Strings may reference the version or the root themselves.
        return strings[key].replace("{{@version}}", ver).replace("{{@root}}", root)

    # Passes until stable, so that partials and strings can contain placeholders themselves.
    for _ in range(4):
        rendered = PLACEHOLDER.sub(substitute, template)
        if rendered == template:
            return rendered
        template = rendered
    sys.exit("error: placeholders nest too deeply")


def sitemap() -> str:
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">',
    ]
    for page, *_ in PAGES.values():
        for _, prefix, *_ in LANGUAGES:
            lines.append(f"  <url><loc>{SITE_URL}{prefix}{page}</loc>")
            for code, alt_prefix, *_ in LANGUAGES:
                lines.append(f'    <xhtml:link rel="alternate" hreflang="{code}" href="{SITE_URL}{alt_prefix}{page}"/>')
            lines.append(f'    <xhtml:link rel="alternate" hreflang="x-default" href="{SITE_URL}/{page}"/>')
            lines.append("  </url>")
    lines.append("</urlset>")
    return "\n".join(lines) + "\n"


def build() -> None:
    strings = load_strings()
    ver = version()
    if DIST.exists():
        shutil.rmtree(DIST)
    DIST.mkdir(parents=True)

    for template_name, (page, title_key, description_key) in PAGES.items():
        template = (SITE / "src" / template_name).read_text()
        for code, prefix, *_ in LANGUAGES:
            out = DIST / prefix.strip("/") / page / "index.html"
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(render(template, code, prefix, page, strings[code], ver, (title_key, description_key)))
    not_found = (SITE / "src" / "404.html").read_text()
    (DIST / "404.html").write_text(render(not_found, "en", "/", "", strings["en"], ver))
    (DIST / "sitemap.xml").write_text(sitemap())
    (DIST / "robots.txt").write_text(f"User-agent: *\nAllow: /\n\nSitemap: {SITE_URL}/sitemap.xml\n")

    assets = DIST / "assets"
    shutil.copytree(SITE / "src" / "assets", assets)
    shutil.copytree(SITE / "shots", assets / "shots")
    # Single sources: the menu bar mark, the brand marks and the code font are the app's own.
    shutil.copy(ROOT / "assets" / "menubar-template.svg", assets / "menubar-icon.svg")
    shutil.copytree(ROOT / "assets" / "brands", assets / "brands")
    fonts = assets / "fonts"
    fonts.mkdir()
    for name in ("AtkinsonHyperlegibleMono-Variable.ttf", "AtkinsonHyperlegibleMono-OFL.txt"):
        shutil.copy(ROOT / "assets" / "fonts" / name, fonts / name)
    shutil.move(assets / "favicon.png", DIST / "favicon.png")
    shutil.copy(SITE / "src" / "_headers", DIST / "_headers")
    shutil.copy(SITE / "wrangler.toml", DIST.parent / "wrangler.toml")
    print(f"Built {DIST.relative_to(ROOT)} (Safay {ver})")


# Source picture -> site/shots base name. The README's pictures in docs/img/ already have clean window corners
# (scripts/readme_images.py); a capture straight from docs/screenshots/ (previews/screenshots.test.tsx) gets them here.
# English only for now: a page in another language falls back to these.
SHOT_SOURCES = {
    "docs/img/panel-light.png": "panel-light-en",
    "docs/screenshots/panel-dark.png": "panel-dark-en",
}
# The hero picture: one window, light on the left of a diagonal and dark on the right. Both captures show the same
# window at the same size, so the halves meet exactly. The cut runs from this share of the width at the top to this
# share at the bottom.
SPLIT_SOURCES = ("docs/img/accounts-light.png", "docs/img/accounts-dark.png", "main-split-en")
SPLIT_TOP, SPLIT_BOTTOM = 0.64, 0.40


def shots() -> None:
    try:
        from PIL import Image, ImageChops, ImageDraw
    except ImportError:
        sys.exit("error: site/build.py --shots needs Pillow: python3 -m pip install Pillow")
    sys.path.insert(0, str(ROOT / "scripts"))
    from readme_images import round_corners

    target = SITE / "shots"
    target.mkdir(exist_ok=True)
    for stale in target.glob("*.webp"):
        stale.unlink()
    for source, name in SHOT_SOURCES.items():
        png = ROOT / source
        if not png.exists():
            sys.exit(f"error: {source} is missing (run make screenshots, then scripts/readme_images.py)")
        image = Image.open(png)
        if source.startswith("docs/screenshots/"):
            image = round_corners(image)
        # Lossy at a high quality keeps text crisp; the alpha channel carries the window's corners.
        image.convert("RGBA").save(target / f"{name}.webp", "WEBP", quality=88, method=6)

    light, dark = (Image.open(ROOT / path).convert("RGBA") for path in SPLIT_SOURCES[:2])
    if light.size != dark.size:
        sys.exit("error: the light and dark captures differ in size; capture them again together")
    width, height = light.size
    # The two captures antialias the window's edge a little differently: one outline for both, the tighter one. More
    # than a sliver of difference means the windows were not the same.
    outline = ImageChops.darker(light.getchannel("A"), dark.getchannel("A"))
    differing = sum(ImageChops.difference(light.getchannel("A"), dark.getchannel("A")).histogram()[33:])
    if differing > width * height // 1000:
        sys.exit("error: the light and dark captures show windows of different shapes; capture them again together")
    # The dark half's mask, drawn at four times the size and scaled down for a smooth edge.
    scale = 4
    mask = Image.new("L", (width * scale, height * scale), 0)
    cut = [(SPLIT_TOP * width * scale, 0), (width * scale, 0), (width * scale, height * scale), (SPLIT_BOTTOM * width * scale, height * scale)]
    ImageDraw.Draw(mask).polygon(cut, fill=255)
    mask = mask.resize((width, height), Image.LANCZOS)
    split = Image.composite(dark, light, mask)
    split.putalpha(outline)
    split.save(target / f"{SPLIT_SOURCES[2]}.webp", "WEBP", quality=90, method=6)

    icon = Image.open(ROOT / "assets" / "icon.png").convert("RGBA")
    for size, name in ((256, "logo.png"), (64, "favicon.png")):
        icon.resize((size, size), Image.LANCZOS).save(SITE / "src" / "assets" / name, optimize=True)
    print(f"Wrote {len(SHOT_SOURCES) + 1} pictures into {target.relative_to(ROOT)} and resized the logo")


if __name__ == "__main__":
    shots() if "--shots" in sys.argv[1:] else build()
