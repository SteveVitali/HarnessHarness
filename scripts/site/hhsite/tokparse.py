"""tokparse — parse assets/tokens.css into {theme: {token: "#rrggbb"}}.

Used by test_tokens.py (REQ-XF-102) and the rasterizer (PNG colours).
Only the three top-level declaration blocks are read: the `:root,
[data-theme="light"]` block (light), the `[data-theme="dark"]` block and
the `prefers-color-scheme` dark fallback (dark — the two must agree).
"""
import re

_DECL = re.compile(r"--([a-z0-9-]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;")


def _block(css, pattern):
    m = re.search(pattern, css, re.S)
    if not m:
        raise ValueError("tokens.css: missing block for " + pattern[:40])
    return m.group(1)


def parse(css_text):
    """Return {'light': {...}, 'dark': {...}}; dark = explicit block,
    verified identical to the no-saved-choice media-query block."""
    css = re.sub(r"/\*.*?\*/", "", css_text, flags=re.S)
    light = dict(_DECL.findall(_block(
        css, r":root,\s*\[data-theme=\"light\"\]\s*\{(.*?)\}")))
    dark = dict(_DECL.findall(_block(
        css, r"\[data-theme=\"dark\"\]\s*\{(.*?)\}")))
    mq = dict(_DECL.findall(_block(
        css,
        r"@media\s*\(prefers-color-scheme:\s*dark\)\s*\{[^{}]*\{(.*?)\}")))
    missing = set(light) ^ set(dark)
    if missing:
        raise ValueError("tokens.css: theme asymmetry " + repr(sorted(missing)))
    extra = set(dark) ^ set(mq)
    diffs = {k for k in dark if (mq.get(k) or "").lower() != dark[k].lower()}
    if extra or diffs:
        raise ValueError("tokens.css: [data-theme=dark] and the media-query "
                         "fallback disagree " + repr(sorted(extra | diffs)))
    return {"light": light, "dark": dark}


def hex_rgb(value):
    return tuple(int(value[i:i + 2], 16) for i in (1, 3, 5))
