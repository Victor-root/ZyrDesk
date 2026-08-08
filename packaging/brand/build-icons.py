#!/usr/bin/env python3
"""Builds the product icons from the two logo sources.

Windows reads several sizes out of one .ico file and picks whichever
fits the place it is drawing: the taskbar, the task manager, alt-tab,
the desktop, the file properties dialog. Shipping only a large one makes
Windows shrink it itself, badly, exactly where the icon is seen most.

Two sources, because reducing a drawing is not the same as simplifying
it. The full logo holds from forty-eight pixels up; below that its two
overlapping screens touch and its Z falls under the pixel, so the small
sizes are drawn from a mark made for them.

Every size is rendered from its own source at four times its final
dimensions and then reduced, which keeps the diagonals of the Z clean
instead of stepped. Each of those renders is handed to the .ico as it
is: letting the file be built from the largest one alone would throw the
whole point away.
"""

import io
import pathlib
import sys

import cairosvg
from PIL import Image

# The sizes Windows actually asks for.
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]

# Below this, the full logo stops being readable and the mark made for
# small sizes takes over.
DETAIL_HOLDS_FROM = 48

# Rendering above the target size and reducing afterwards is what keeps
# the small sizes readable; rendering straight at 16 pixels loses the
# thin edges entirely.
OVERSAMPLING = 4

HERE = pathlib.Path(__file__).resolve().parent


def drawn(source: pathlib.Path, size: int) -> Image.Image:
    """The logo at that size, rendered large then reduced."""
    wide = size * OVERSAMPLING
    painted = cairosvg.svg2png(url=str(source), output_width=wide, output_height=wide)
    image = Image.open(io.BytesIO(painted)).convert("RGBA")
    return image.resize((size, size), Image.LANCZOS)


def main() -> int:
    full = HERE / "zyrdesk.svg"
    small = HERE / "zyrdesk-petit.svg"
    for source in (full, small):
        if not source.is_file():
            print(f"logo introuvable : {source}", file=sys.stderr)
            return 1

    layers = [
        drawn(full if size >= DETAIL_HOLDS_FROM else small, size) for size in SIZES
    ]
    by_size = dict(zip(SIZES, layers))

    # The largest carries the file; the others ride along and are used
    # as they are, each at its own size.
    icon = HERE / "zyrdesk.ico"
    by_size[256].save(
        icon,
        format="ICO",
        sizes=[(s, s) for s in SIZES],
        append_images=[by_size[s] for s in SIZES if s != 256],
    )
    print(f"icône écrite : {icon} ({', '.join(str(s) for s in SIZES)})")

    # A large flat image, for anything that cannot read an .ico: the
    # installer, the interface, the documentation.
    portrait = HERE / "zyrdesk-256.png"
    by_size[256].save(portrait, format="PNG")
    print(f"image écrite : {portrait}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
