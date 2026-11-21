#!/usr/bin/env python3
"""Generate the Tauri icon set (PNG sizes + .ico + .icns) from app-icon.png.

Usage:
    python3 scripts/gen_icons.py

Reads app-icon.png (1024x1024 recommended) from the project root and writes
into src-tauri/icons/. If Pillow is available the source image is resized
properly; otherwise the source PNG bytes are embedded as-is (valid, but not
optimized for small sizes).
"""

import io
import os
import struct
import sys
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "app-icon.png")
OUT = os.path.join(ROOT, "src-tauri", "icons")


def load_source():
    if os.path.exists(SRC):
        with open(SRC, "rb") as f:
            data = f.read()
        return data
    return None


def resize_png(data, size):
    """Resize a PNG to size x size, falling back to the original bytes."""
    try:
        from PIL import Image

        img = Image.open(io.BytesIO(data)).convert("RGBA")
        img = img.resize((size, size), Image.LANCZOS)
        buf = io.BytesIO()
        img.save(buf, "PNG")
        return buf.getvalue()
    except Exception:
        return data


def png_chunk(tag, payload):
    c = struct.pack(">I", len(payload)) + tag + payload
    crc = zlib.crc32(tag + payload) & 0xFFFFFFFF
    return c + struct.pack(">I", crc)


def solid_png(size):
    """Pure-python fallback: solid dark square PNG."""
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    row = b"\x00" + b"\x18\x18\x1b\xff" * size
    idat = zlib.compress(row * size, 9)
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", idat)
        + png_chunk(b"IEND", b"")
    )


def placeholder_png(size):
    """Draw a simple placeholder icon when no source image exists."""
    try:
        from PIL import Image, ImageDraw

        img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        d = ImageDraw.Draw(img)
        r = size // 5
        d.rounded_rectangle([0, 0, size - 1, size - 1], radius=r, fill=(24, 24, 27, 255))
        m = size // 4
        b = max(2, size // 32)
        green = (16, 185, 129, 255)
        white = (250, 250, 250, 255)
        d.line([(m, m + b * 6), (m, m), (m + b * 6, m)], fill=white, width=b)
        d.line([(size - m - b * 6, m), (size - m, m), (size - m, m + b * 6)], fill=white, width=b)
        d.line([(m, size - m - b * 6), (m, size - m), (m + b * 6, size - m)], fill=white, width=b)
        d.line([(size - m - b * 6, size - m), (size - m, size - m), (size - m, size - m - b * 6)], fill=white, width=b)
        c = size // 2
        rad = size // 8
        d.ellipse([c - rad, c - rad, c + rad, c + rad], fill=green)
        buf = io.BytesIO()
        img.save(buf, "PNG")
        return buf.getvalue()
    except Exception:
        return solid_png(size)


def write_ico(sizes_png, path):
    """Write a Windows .ico containing PNG-compressed entries (Vista+)."""
    entries = b""
    offset = 6 + 16 * len(sizes_png)
    blobs = b""
    for size, data in sizes_png:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    with open(path, "wb") as f:
        f.write(struct.pack("<HHH", 0, 1, len(sizes_png)) + entries + blobs)


def write_icns(png_by_size, path):
    """Write a macOS .icns with PNG-encoded chunks (ic07..ic10)."""
    codes = [(128, b"ic07"), (256, b"ic08"), (512, b"ic09"), (1024, b"ic10")]
    chunks = b""
    for size, code in codes:
        data = png_by_size.get(size)
        if data is None:
            continue
        chunks += code + struct.pack(">I", len(data) + 8) + data
    total = 8 + len(chunks)
    with open(path, "wb") as f:
        f.write(b"icns" + struct.pack(">I", total) + chunks)


def main():
    os.makedirs(OUT, exist_ok=True)
    src = load_source()
    if src is None:
        print("app-icon.png not found, generating a placeholder icon")
        src = placeholder_png(1024)

    png_targets = [
        (32, "32x32.png"),
        (128, "128x128.png"),
        (256, "128x128@2x.png"),
        (512, "icon.png"),
    ]
    png_by_size = {}
    for size, name in png_targets:
        data = resize_png(src, size)
        png_by_size[size] = data
        with open(os.path.join(OUT, name), "wb") as f:
            f.write(data)
        print("wrote", name)

    png_by_size[1024] = resize_png(src, 1024)

    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    write_ico([(s, resize_png(src, s)) for s in ico_sizes], os.path.join(OUT, "icon.ico"))
    print("wrote icon.ico")

    write_icns(png_by_size, os.path.join(OUT, "icon.icns"))
    print("wrote icon.icns")

    print("icon set generated in", OUT)


if __name__ == "__main__":
    sys.exit(main())
