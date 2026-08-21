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

Every size is drawn at its own size, and none is reduced from a larger
one. Rendering at four times and reducing afterwards is the usual advice
and it was followed here; put side by side at the sizes that matter, it
loses. The renderer works out how much of each pixel a stroke covers and
lays down exactly that, which is as good as an edge gets; reducing a
larger image asks a filter to guess the same thing from pixels that have
already thrown the answer away, and the guess is softer every time.

And the .ico is written here rather than left to the imaging library,
for the one reason that made all of the above pointless. That library
stores every size as a PNG, and Windows only reads a PNG out of an .ico
at 256 pixels: below that it wants the bitmap the format has carried
since 1985. Twenty carefully drawn sizes were therefore being skipped
over, Windows fell back to the only entry it could read, and shrank the
256 down to 42 for the taskbar itself. The icon was blurred next to every
other icon on the bar, and nothing about the drawing was going to fix it.
"""

import io
import pathlib
import struct
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
    | {128, 256}
)

# The sizes the notification area asks for, which cannot read an .ico:
# the tray is handed one image and Windows scales it to whatever the bar
# is drawing at. So it is handed the right one instead, and these are
# written out on their own for the program to carry.
#
# Sixteen logical pixels at each of the usual scalings, which is what
# `GetSystemMetrics(SM_CXSMICON)` answers.
TRAY = [16 * scale // 100 for scale in (100, 125, 150, 175, 200, 250)]

HERE = pathlib.Path(__file__).resolve().parent


def drawn(source: pathlib.Path, size: int) -> Image.Image:
    """The logo drawn at that size, and never reduced from a larger one."""
    painted = cairosvg.svg2png(url=str(source), output_width=size, output_height=size)
    return Image.open(io.BytesIO(painted)).convert("RGBA")


def bitmap(image: Image.Image) -> bytes:
    """One size of an .ico, the way Windows reads it below 256 pixels.

    A header, the pixels bottom-up in blue-green-red-alpha order, and the
    transparency mask. The mask is a bit per pixel and predates images
    having an alpha channel at all; Windows goes by the alpha nowadays,
    but a file without a mask is not an icon file.
    """
    wide, high = image.size
    header = struct.pack(
        "<IiiHHIIiiII",
        40,  # the size of this header
        wide,
        high * 2,  # the colours and the mask, stacked
        1,  # planes
        32,  # bits per pixel
        0,  # not compressed
        0,  # let the reader work the size out
        0,
        0,  # pixels per metre, which an icon has no opinion on
        0,
        0,  # every colour is used and every colour matters
    )

    across = wide * 4
    upside_down = image.tobytes("raw", "BGRA")
    colours = b"".join(
        upside_down[y * across : (y + 1) * across] for y in range(high - 1, -1, -1)
    )

    # A row of the mask is padded to four bytes, as every row of every
    # bitmap in this format is.
    padded = ((wide + 31) // 32) * 4
    packed = (wide + 7) // 8
    holes = (
        image.getchannel("A")
        .point(lambda value: 255 if value == 0 else 0)
        .convert("1", dither=Image.Dither.NONE)
        .tobytes()
    )
    mask = b"".join(
        holes[y * packed : (y + 1) * packed].ljust(padded, b"\0")
        for y in range(high - 1, -1, -1)
    )
    return header + colours + mask


def write_icon(path: pathlib.Path, by_size: dict[int, Image.Image]) -> None:
    """Writes the .ico, each size in the form Windows can read at it."""
    sizes = sorted(by_size)
    # 256 goes in as a PNG, which is the one size Windows reads that way
    # and what keeps the file from being a megabyte of raw pixels.
    bodies = []
    for size in sizes:
        if size >= 256:
            large = io.BytesIO()
            by_size[size].save(large, format="PNG")
            bodies.append(large.getvalue())
        else:
            bodies.append(bitmap(by_size[size]))

    at = 6 + 16 * len(sizes)
    listing = b""
    for size, body in zip(sizes, bodies):
        # Nought means 256 here: the field is one byte wide and the
        # format is older than screens that large.
        listing += struct.pack(
            "<BBBBHHII", size & 0xFF, size & 0xFF, 0, 0, 1, 32, len(body), at
        )
        at += len(body)

    path.write_bytes(struct.pack("<HHH", 0, 1, len(sizes)) + listing + b"".join(bodies))


def main() -> int:
    logo = HERE / "zyrdesk.svg"
    if not logo.is_file():
        print(f"logo introuvable : {logo}", file=sys.stderr)
        return 1

    by_size = {size: drawn(logo, size) for size in sorted(set(SIZES) | set(TRAY))}

    icon = HERE / "zyrdesk.ico"
    write_icon(icon, {size: by_size[size] for size in SIZES})
    print(
        f"icône écrite : {icon} ({icon.stat().st_size // 1024} Ko, "
        f"{', '.join(str(s) for s in SIZES)})"
    )

    # A large flat image, for anything that cannot read an .ico: the
    # installer, the interface, the documentation.
    portrait = HERE / "zyrdesk-256.png"
    by_size[256].save(portrait, format="PNG")
    print(f"image écrite : {portrait}")

    for size in TRAY:
        beside_the_clock = HERE / f"zyrdesk-{size}.png"
        by_size[size].save(beside_the_clock, format="PNG")
    print(f"zone de notification : {', '.join(str(s) for s in TRAY)}")

    # And the drawing itself where the interface reads it. Copied rather
    # than kept in two places: two logos for one product is what the eye
    # catches first, and two files nobody compares is how that happens.
    page = HERE.parents[1] / "crates" / "zyr-ui" / "web" / "zyrdesk.svg"
    page.write_text(logo.read_text(encoding="utf-8"), encoding="utf-8")
    print(f"dessin recopié : {page}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
