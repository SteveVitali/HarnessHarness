"""contrast — WCAG 2 relative luminance and contrast ratio.

Reimplemented from the WCAG formula (design/39 F2 requires the committed
test to reimplement the scratch contrast.py and cite the formula):
https://www.w3.org/TR/WCAG22/#dfn-relative-luminance — sRGB channel
linearisation then L = 0.2126R + 0.7152G + 0.0722B; ratio (L1+0.05)/(L2+0.05).
"""
from .tokparse import hex_rgb


def _lin(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def luminance(hexcolor):
    r, g, b = hex_rgb(hexcolor)
    return 0.2126 * _lin(r) + 0.7152 * _lin(g) + 0.0722 * _lin(b)


def ratio(fg_hex, bg_hex):
    l1, l2 = luminance(fg_hex), luminance(bg_hex)
    if l1 < l2:
        l1, l2 = l2, l1
    return (l1 + 0.05) / (l2 + 0.05)
