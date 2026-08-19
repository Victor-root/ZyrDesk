#!/usr/bin/env python3
"""Builds the product icons from the logo.

Windows reads several sizes out of one .ico file and picks whichever
fits the place it is drawing: the taskbar, the title bar, the task
manager, alt-tab, the desktop, the file properties dialog. Shipping only
a large one makes Windows shrink it itself, badly, exactly where the
icon is seen most.

One source for every size. A mark drawn for small sizes used to take
over below forty-eight pixels, on the grounds that reducing a drawing is
not the same as simplifying it. That is true and it was still the wrong
call: it put a different logo in the taskbar and the title bar from the
one beside the clock and inside the window, and two logos for one
product is what the eye catches first. A logo is recognised before it is
read.

Every size is rendered at four times its final dimensions and then
reduced, which keeps the diagonals of the Z clean instead of stepped.
Each of those renders is handed to the .ico as it is: letting the file
be built from the largest one alone would throw the whole point away.
"""

import io
import pathlib
import sys

import cairosvg
from PIL import Image

# The sizes Windows actually asks for.
#
# Not a round handful. Windows asks for a logical size, 16 for a title
# bar, 24 for the taskbar, 32 and 48 for the explorer, and multiplies it
# by the scaling of the screen it is drawing on: 125, 150, 175 and 200
# per cent are all ordinary. A screen at 175 per cent asks the taskbar
# icon at 42 pixels, and a file that stops at 40 makes Windows stretch
# the 40 by two pixels, or fall back to the 32 and stretch that. Either
# way the diagonals of the Z go to staircases, on the one icon that is
# under the eye all day.
#
# So every product of the four logical sizes by the six usual scalings,
# plus the large ones the explorer wants.
SIZES = sorted(
    {
        logical * scale // 100
        for logical in (16, 24, 32, 48)
        for scale in (100, 125, 150, 175, 200, 250)
    }
    | {128, 192, 256}
)

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
    logo = HERE / "zyrdesk.svg"
    if not logo.is_file():
        print(f"logo introuvable : {logo}", file=sys.stderr)
        return 1

    by_size = {size: drawn(logo, size) for size in SIZES}

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
