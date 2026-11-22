#!/usr/bin/env python3
"""Check WCAG contrast of the semantic color pairs in tokens.css (stdlib only).

Usage: python3 docs/design-system/check_contrast.py [path/to/tokens.css]
Exit status 1 when a required pair misses its bar. Pairs marked "info" are
printed for reference and never fail (decorative edges, disabled controls,
which WCAG exempts).

What is covered, beyond text on the base surfaces: status icons, edges, focus
and the check ring on every surface a row or node can take (hover, selected,
sunken); the amber bar and its inverted button; text and the status tick on
tinted fills; the edge of every control (border-strong) on its surfaces; and
blended pairs, where an element is drawn at an opacity below 1 (the foreground
is mixed into the background in sRGB first, as the browser does).
"""
import math, re, sys
from pathlib import Path

path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).with_name("tokens.css")
css = path.read_text()


def block(pattern):
    m = re.search(pattern + r"\s*\{", css)
    i, depth = m.end(), 1
    while depth:
        depth += {"{": 1, "}": -1}.get(css[i], 0)
        i += 1
    return css[m.end():i - 1]


def decls(text):
    return dict(re.findall(r"(--[\w-]+):\s*([^;]+);", text))


prim = decls(block(r"@layer primitive"))
sem = decls(block(r"@layer semantic"))  # theme-independent roles (opacity, ...)
light = decls(block(r':root,\s*:root\[data-theme="light"\]'))
dark = decls(block(r':root\[data-theme="dark"\]'))
assert dark == decls(block(r':root:not\(\[data-theme="light"\]\)')), \
    "system-dark block differs from data-theme dark block"


def resolve(v, env):
    while "var(" in v:
        v = re.sub(r"var\((--[\w-]+)\)", lambda m: env[m.group(1)], v)
    return v.strip()


def linear_rgb(lch):  # oklch -> linear sRGB, clamped (no gamut mapping)
    L, C, h = lch
    a, b = C * math.cos(math.radians(h)), C * math.sin(math.radians(h))
    l, m, s = ((L + 0.3963377774 * a + 0.2158037573 * b) ** 3,
               (L - 0.1055613458 * a - 0.0638541728 * b) ** 3,
               (L - 0.0894841775 * a - 1.2914855480 * b) ** 3)
    rgb = (4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
           -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
           -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s)
    return tuple(min(1, max(0, x)) for x in rgb)


def encode(x):
    return 12.92 * x if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055


def decode(x):
    return x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4


def blend(fg, bg, alpha):  # CSS opacity mixes in gamma-encoded sRGB
    return tuple(decode(alpha * encode(f) + (1 - alpha) * encode(b)) for f, b in zip(fg, bg))


def luminance(rgb):
    return 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]


def ratio(a, b):
    hi, lo = sorted((luminance(a), luminance(b)), reverse=True)
    return (hi + 0.05) / (lo + 0.05)


SURF = ["surface", "page", "raised", "sunken", "hover", "selected"]
CTRL = ["surface", "page", "raised", "sunken", "hover"]  # where a control edge is drawn
# (group, fg, bg, bar, required, opacity of fg or None, opacity token for fg)
checks = []


def add(group, fgs, bgs, bar, required=True, opacity=None):
    for fg in fgs:
        for bg in bgs:
            checks.append((group, fg, bg, bar, required, opacity))


add("text, muted, faint on surfaces", ["text", "text-muted", "text-faint"], SURF, 4.5)
add("text, muted on danger-soft (error notice)", ["text", "text-muted"], ["danger-soft"], 4.5)
add("accent-text on surfaces and accent-soft", ["accent-text"], SURF + ["accent-soft"], 4.5)
add("on-accent on accent, accent-hover (keycap too)", ["on-accent"], ["accent", "accent-hover"], 4.5)
add("danger on surfaces and danger-soft", ["danger"], ["surface", "page", "raised", "danger-soft"], 4.5)
add("on-attention on attention, attention-hover", ["on-attention"], ["attention", "attention-hover"], 4.5)
add("attention on on-attention (inverted bar button)", ["attention", "attention-hover"], ["on-attention"], 4.5)
add("each status color on surfaces", ["status-" + s for s in
    ("draft", "ready", "available", "active", "done", "blocked", "abandoned")], SURF, 3.0)
add("edge, edge-hot, accent, focus on surfaces", ["edge", "edge-hot", "accent", "focus"], SURF, 3.0)
add("check-ring on surfaces", ["check-ring"], SURF, 3.0)
add("tick (surface) on done fill", ["surface"], ["status-done"], 3.0)
add("border-strong (control edge) on surfaces", ["border-strong"], CTRL, 3.0)
# Reference only.
add("border-quiet (decorative edge) on surface", ["border-quiet"], ["surface"], 3.0, required=False)
add("border (hairline separator) on surface", ["border"], ["surface"], 3.0, required=False)
add("disabled primary button text at opacity-disabled (exempt)", ["on-accent"], ["accent"], 4.5,
    required=False, opacity="--opacity-disabled")
add("dimmed text at opacity-dimmed on surface (do not use for text)", ["text-muted"], ["surface"], 4.5,
    required=False, opacity="--opacity-dimmed")

failed = 0
summary = {}
for name, theme in (("light", light), ("dark", dark)):
    env = {**prim, **sem, **theme}
    col = {}
    for k, v in theme.items():
        r = resolve(v, env)
        m = re.fullmatch(r"oklch\(([\d.]+)\s+([\d.]+)\s+([\d.]+)\)", r)
        if k.startswith("--color-") and m:
            col[k[8:]] = linear_rgb(tuple(float(x) for x in m.groups()))
    print(f"== {name} ==")
    for group, fg, bg, bar, required, opacity in checks:
        if not required and (fg not in col or bg not in col):
            continue  # a reference pair for a token this file does not define
        fgc = col[fg]
        if opacity:
            fgc = blend(fgc, col[bg], float(resolve(env[opacity], env)))
        r = ratio(fgc, col[bg])
        status = "ok  " if r >= bar else ("MISS" if required else "info")
        failed += status == "MISS"
        tag = f" @{opacity[2:]}" if opacity else ""
        print(f"  {status} {fg + tag:28} on {bg:12} {r:5.2f} (>= {bar})")
        worst = summary.setdefault((group, name), (99, ""))
        if r < worst[0]:
            summary[(group, name)] = (r, f"{fg} on {bg}")

print("\n== worst pair per group ==")
for group in dict.fromkeys(g for g, _, _, _, _, _ in checks):
    lo = summary.get((group, "light"), (0, "n/a"))
    dk = summary.get((group, "dark"), (0, "n/a"))
    print(f"  {group}\n      light {lo[0]:5.2f} ({lo[1]})   dark {dk[0]:5.2f} ({dk[1]})")
print(f"\n{failed} required pair(s) below the bar" if failed else "\nall required pairs pass")
sys.exit(1 if failed else 0)
