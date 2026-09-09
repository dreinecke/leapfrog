#!/usr/bin/env python3
"""Build the local Omarchy theme gallery data and image thumbnails."""

from __future__ import annotations

import json
import re
import tomllib
from pathlib import Path

from PIL import Image, ImageOps


ROOT = Path(__file__).resolve().parent
STOCK_THEMES = Path("/usr/share/omarchy/themes")
USER_THEMES = Path.home() / ".config/omarchy/themes"
USER_BACKGROUNDS = Path.home() / ".config/omarchy/backgrounds"
CURRENT_THEME_NAME = Path.home() / ".local/state/omarchy/current/theme.name"
ASSETS = ROOT / "assets"
IMAGE_EXTENSIONS = {".avif", ".jpeg", ".jpg", ".png", ".webp"}
HEX_COLOR = re.compile(r"#[0-9a-fA-F]{6}")
LEADING_NUMBER = re.compile(r"^\d+[-_ ]*")
NON_SLUG = re.compile(r"[^a-z0-9]+")

COLOR_ORDER = [
    "accent",
    "selection",
    "selection_background",
    "selection_foreground",
    "muted",
    "background",
    "bg",
    "dark_background",
    "dark_bg",
    "darker_background",
    "darker_bg",
    "lighter_background",
    "lighter_bg",
    "foreground",
    "fg",
    "dark_foreground",
    "dark_fg",
    "light_foreground",
    "light_fg",
    "bright_foreground",
    "bright_fg",
    "cursor",
    "red",
    "orange",
    "yellow",
    "green",
    "cyan",
    "blue",
    "magenta",
    "brown",
    "bright_red",
    "bright_orange",
    "bright_yellow",
    "bright_green",
    "bright_cyan",
    "bright_blue",
    "bright_magenta",
]


def slugify(value: str) -> str:
    return NON_SLUG.sub("-", value.lower()).strip("-")


def pretty(value: str) -> str:
    return " ".join(part.capitalize() for part in re.split(r"[-_]", value) if part)


def background_name(path: Path) -> str:
    stem = LEADING_NUMBER.sub("", path.stem)
    return pretty(stem) or path.name


def read_theme(path: Path) -> dict[str, object]:
    color_file = path / "colors.toml"
    if not color_file.is_file():
        return {}
    with color_file.open("rb") as handle:
        return tomllib.load(handle)


def color_items(values: dict[str, object]) -> list[dict[str, str]]:
    colors = {
        key: value.lower()
        for key, value in values.items()
        if isinstance(value, str) and HEX_COLOR.fullmatch(value)
    }
    order = {key: index for index, key in enumerate(COLOR_ORDER)}
    keys = sorted(colors, key=lambda key: (order.get(key, len(order)), key))
    return [{"key": key, "value": colors[key]} for key in keys]


def first_color(values: dict[str, object], *keys: str, fallback: str) -> str:
    for key in keys:
        value = values.get(key)
        if isinstance(value, str) and HEX_COLOR.fullmatch(value):
            return value.lower()
    return fallback


def image_files(directory: Path) -> list[Path]:
    if not directory.is_dir():
        return []
    return sorted(
        path
        for path in directory.iterdir()
        if path.is_file() and path.suffix.lower() in IMAGE_EXTENSIONS
    )


def make_thumbnail(source: Path, destination: Path) -> bool:
    try:
        with Image.open(source) as opened:
            image = ImageOps.exif_transpose(opened).convert("RGB")
            thumbnail = ImageOps.fit(
                image,
                (640, 360),
                method=Image.Resampling.LANCZOS,
                centering=(0.5, 0.5),
            )
            destination.parent.mkdir(parents=True, exist_ok=True)
            thumbnail.save(destination, "WEBP", quality=82, method=6)
        return True
    except (OSError, ValueError) as error:
        print(f"Skipping unreadable image {source}: {error}")
        return False


def collect_theme(slug: str, current_slug: str) -> dict[str, object]:
    stock = STOCK_THEMES / slug
    user = USER_THEMES / slug

    values: dict[str, object] = {}
    if stock.is_dir():
        values.update(read_theme(stock))
    if user.is_dir():
        values.update(read_theme(user))

    sources = [
        (stock / "backgrounds", "Stock"),
        (user / "backgrounds", "Theme"),
        (USER_BACKGROUNDS / slug, "Added"),
    ]
    backgrounds: list[dict[str, str]] = []
    seen: set[Path] = set()
    index = 0
    for directory, source_kind in sources:
        for source in image_files(directory):
            resolved = source.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            index += 1
            output = ASSETS / slug / f"{index:02d}-{slugify(source.stem)}.webp"
            if make_thumbnail(source, output):
                backgrounds.append(
                    {
                        "name": background_name(source),
                        "thumbnail": output.relative_to(ROOT).as_posix(),
                        "source": source_kind,
                    }
                )

    colors = color_items(values)
    return {
        "slug": slug,
        "name": pretty(slug),
        "current": slug == current_slug,
        "mode": values.get("mode", "unknown"),
        "cover": backgrounds[0]["thumbnail"] if backgrounds else None,
        "backgrounds": backgrounds,
        "colors": colors,
        "ui": {
            "accent": first_color(values, "accent", "blue", fallback="#7aa2f7"),
            "background": first_color(
                values, "background", "bg", fallback="#17191f"
            ),
            "surface": first_color(
                values,
                "lighter_background",
                "lighter_bg",
                "selection",
                fallback="#252832",
            ),
        },
    }


def main() -> None:
    ASSETS.mkdir(parents=True, exist_ok=True)
    current_name = (
        CURRENT_THEME_NAME.read_text(encoding="utf-8").strip()
        if CURRENT_THEME_NAME.is_file()
        else ""
    )
    current_slug = slugify(current_name)

    slugs = {
        path.name
        for root in (STOCK_THEMES, USER_THEMES)
        if root.is_dir()
        for path in root.iterdir()
        if path.is_dir() and not path.name.startswith(".")
    }
    themes = [collect_theme(slug, current_slug) for slug in sorted(slugs)]
    payload = {"current": current_slug, "themes": themes}
    output = "window.OMARCHY_THEME_DATA = " + json.dumps(
        payload, ensure_ascii=False, separators=(",", ":")
    ) + ";\n"
    (ROOT / "themes.js").write_text(output, encoding="utf-8")

    background_count = sum(len(theme["backgrounds"]) for theme in themes)
    color_count = sum(len(theme["colors"]) for theme in themes)
    print(
        f"Generated {len(themes)} themes, {background_count} background thumbnails, "
        f"and {color_count} swatches."
    )


if __name__ == "__main__":
    main()
