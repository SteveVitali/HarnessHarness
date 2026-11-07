"""raster — deterministic PNG writer + the double-frame glyph rasterizer
(REQ-XF-107). Stdlib `zlib`/`struct` only; no timestamps, fixed zlib
level, byte-stable across builds.

Glyph geometry (design/39 §Wordmark): a 32-unit canvas; two concentric
square frames — outer 24 units at offset 4, inner 16 units at offset 8 —
with 2-unit strokes; the needle is a 2-unit bar at x=15 spanning y=2..22.
The geometry is rectangles only, so pixels are exact.
"""
import struct
import zlib

CANVAS = 32
OUTER = (4, 24)   # offset, size
INNER = (8, 16)
NEEDLE = (15, 2, 2, 20)  # x, y, w, h
STROKE = 2


def _ring(u, v, off, size, stroke):
    return (off <= u < off + size and off <= v < off + size and
            (u < off + stroke or u >= off + size - stroke or
             v < off + stroke or v >= off + size - stroke))


def glyph_pixel(u, v):
    """'ink' | 'needle' | None at unit coords (u, v) in [0, 32)."""
    nx, ny, nw, nh = NEEDLE
    if nx <= u < nx + nw and ny <= v < ny + nh:
        return "needle"
    if _ring(u, v, *OUTER, STROKE) or _ring(u, v, *INNER, STROKE):
        return "ink"
    return None


def paint_glyph(buf, img_w, img_h, ox, oy, scale, ink_rgba, needle_rgba):
    """Paint the glyph into an RGBA bytearray at pixel offset (ox, oy),
    `scale` device pixels per unit. Pixels are unit-exact (rectangles
    only), so output is byte-stable."""
    for py in range(img_h):
        for px in range(img_w):
            u = (px - ox) / scale
            v = (py - oy) / scale
            if not (0 <= u < CANVAS and 0 <= v < CANVAS):
                continue
            kind = glyph_pixel(u, v)
            if kind is None:
                continue
            rgba = needle_rgba if kind == "needle" else ink_rgba
            i = (py * img_w + px) * 4
            buf[i:i + 4] = bytes(rgba)


def write_png(width, height, rgba):
    """RGBA bytearray -> PNG bytes. 8-bit RGBA, filter 0 per scanline,
    fixed compression — deterministic."""
    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data +
                struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))
    raw = b"".join(
        b"\x00" + bytes(rgba[y * width * 4:(y + 1) * width * 4])
        for y in range(height))
    return (b"\x89PNG\r\n\x1a\n" +
            chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(raw, 9)) +
            chunk(b"IEND", b""))


def favicon_png(tokens, size=64):
    """Transparent-ground glyph PNG; frames in --ink, needle in --needle
    (light-theme token values — the PNG is theme-fixed, favicon.svg
    carries the media query)."""
    buf = bytearray(size * size * 4)
    ink = tuple(int(tokens["light"]["ink"][i:i + 2], 16) for i in (1, 3, 5)) + (255,)
    needle = tuple(int(tokens["light"]["needle"][i:i + 2], 16) for i in (1, 3, 5)) + (255,)
    paint_glyph(buf, size, size, 0, 0, size / CANVAS, ink, needle)
    return write_png(size, size, buf)


def og_png(tokens, width=1200, height=630, glyph_size=288):
    """1200×630 social image: glyph centred on --paper, no numbers, no
    text (design/39 F13 — a card that can never go stale)."""
    paper = tuple(int(tokens["light"]["paper"][i:i + 2], 16) for i in (1, 3, 5)) + (255,)
    buf = bytearray(paper * (width * height))
    ink = tuple(int(tokens["light"]["ink"][i:i + 2], 16) for i in (1, 3, 5)) + (255,)
    needle = tuple(int(tokens["light"]["needle"][i:i + 2], 16) for i in (1, 3, 5)) + (255,)
    ox = (width - glyph_size) // 2
    oy = (height - glyph_size) // 2
    paint_glyph(buf, width, height, ox, oy, glyph_size / CANVAS, ink, needle)
    return write_png(width, height, buf)
