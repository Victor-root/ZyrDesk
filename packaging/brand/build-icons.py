#!/usr/bin/env python3
"""Builds the product icons from the single logo source.

Windows reads several sizes out of one .ico file and picks whichever
fits the place it is drawing: the taskbar, the task manager, the
desktop, the file properties dialog. Shipping only a large one makes
Windows shrink it itself, badly, exactly where the icon is seen most.

Every size is rendered from the SVG at four times its final dimensions
and then reduced, which keeps the diagonals of the Z clean instead of
stepped.
"""

import io
import pathlib
import sys

import cairosvg
from PIL import Image

# The sizes Windows actually asks for.
SIZES = [16, 24, 32, 48, 64, 128, 256]

# Rendering above the target size and reducing afterwards is what keeps
# the small sizes readable; rendering straight at 16 pixels loses the
# thin edges entirely.
OVERSAMPLING = 4

HERE = pathlib.Path(__file__).resolve().parent


def drawn(source: pathlib.Path, size: int) -> Image.Image:
    """The logo at that size, rendered large then reduced."""
    wide = size * OVERSAMPLING
    painted = cairosvg.svg2png(
        url=str(source), output_width=wide, output_height=wide
    )
    image = Image.open(io.BytesIO(painted)).convert("RGBA")
    if size == wide:
        return image
    return image.resize((size, size), Image.LANCZOS)


def main() -> int:
    source = HERE / "zyrdesk.svg"
    if not source.is_file():
        print(f"logo introuvable : {source}", file=sys.stderr)
        return 1

    layers = [drawn(source, size) for size in SIZES]

    icon = HERE / "zyrdesk.ico"
    layers[-1].save(icon, format="ICO", sizes=[(s, s) for s in SIZES])
    print(f"icône écrite : {icon} ({', '.join(str(s) for s in SIZES)})")

    # A large flat image, for anything that cannot read an .ico: the
    # installer, the interface, the documentation.
    portrait = HERE / "zyrdesk-256.png"
    layers[SIZES.index(256)].save(portrait, format="PNG")
    print(f"image écrite : {portrait}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
