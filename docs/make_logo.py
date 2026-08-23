#!/usr/bin/env python3
"""Generate the persistex logo, in SVG and PNG, light and dark.

The waveform in the mark is a real phase-optimised multisine (4 harmonics,
RPF ~1.02) drawn between its peak-limit guides -- the same figure the app icon
uses. Run from anywhere:

    python3 docs/make_logo.py

SVG output needs only the standard library; the PNG fallbacks need Pillow.
"""
import math
import os

BLUE = "#2f6fd0"
MARK, INSET, AMP, STROKE = 132.0, 24.0, 26.0, 6.5
GAP, PAD_X, HEIGHT = 30.0, 8.0, 150.0
TITLE_SIZE, TAG_SIZE = 52.0, 17.0
TAGLINE = "multisine excitation design for system identification"
FONT_STACK = "Helvetica Neue, Helvetica, Arial, sans-serif"

# A real phase-optimised multisine: harmonics 1-4 at unit amplitude, phases from
# the tool's own swap+lp optimiser (RPF 1.020). Frozen here so generating the logo
# needs nothing but the standard library.
HARMONICS = [1, 2, 3, 4]
PHASES = [5.471247481863848, 4.457879318104374, 1.746488089108557, 3.343543680428522]


def waveform(points=256):
    """Direct summation -- only four tones, so no FFT is needed."""
    wave = []
    for n in range(points):
        total = 0.0
        for k, phi in zip(HARMONICS, PHASES):
            total += math.sin(2.0 * math.pi * k * n / points + phi)
        wave.append(total)
    peak = max(abs(v) for v in wave)
    wave = [v / peak for v in wave]

    high, low = max(wave), min(wave)
    rms = math.sqrt(math.fsum(v * v for v in wave) / len(wave))
    rpf = (high - low) / (2.0 * math.sqrt(2.0) * rms)
    return wave, rpf


def trace_points(wave, ox=0.0, oy=0.0):
    n = len(wave)
    return [(ox + INSET + (MARK - 2 * INSET) * i / (n - 1), oy + MARK / 2 - wave[i] * AMP)
            for i in range(n)]


def text_width(size, text):
    """Rough advance width, good enough to size the canvas without a font engine."""
    return sum(0.62 if c not in "iljt.,' " else 0.30 for c in text) * size


def canvas_width():
    text = max(text_width(TITLE_SIZE, "persistex"), text_width(TAG_SIZE, TAGLINE))
    return math.ceil(PAD_X + MARK + GAP + text + PAD_X)


def write_svg(path, title_fill, tag_fill):
    width = canvas_width()
    wave, _ = WAVE
    ox, oy = PAD_X, (HEIGHT - MARK) / 2
    guides = "".join(
        '\n    <line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" stroke="#ffffff" '
        'stroke-opacity="0.42" stroke-width="2"/>'
        % (ox + INSET, oy + MARK / 2 - s * AMP, ox + MARK - INSET, oy + MARK / 2 - s * AMP)
        for s in (1, -1))
    path_d = "M " + " L ".join("%.2f %.2f" % p for p in trace_points(wave, ox, oy))
    text_x = ox + MARK + GAP
    with open(path, "w") as fh:
        fh.write(
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{HEIGHT:.0f}" '
            f'viewBox="0 0 {width} {HEIGHT:.0f}" role="img" aria-label="persistex">\n'
            f'  <title>persistex</title>\n'
            f'  <g>\n'
            f'    <rect x="{ox:.1f}" y="{oy:.1f}" width="{MARK:.0f}" height="{MARK:.0f}" '
            f'rx="30" fill="{BLUE}"/>{guides}\n'
            f'    <path d="{path_d}" fill="none" stroke="#ffffff" stroke-width="{STROKE}" '
            f'stroke-linecap="round" stroke-linejoin="round"/>\n'
            f'  </g>\n'
            f'  <g font-family="{FONT_STACK}">\n'
            f'    <text x="{text_x:.1f}" y="{HEIGHT/2 + 2:.1f}" font-size="{TITLE_SIZE:.0f}" '
            f'font-weight="600" letter-spacing="-1" fill="{title_fill}">persistex</text>\n'
            f'    <text x="{text_x + 2:.1f}" y="{HEIGHT/2 + 30:.1f}" font-size="{TAG_SIZE:.0f}" '
            f'fill="{tag_fill}">{TAGLINE}</text>\n'
            f'  </g>\n'
            f'</svg>\n')


def write_mark(path):
    wave, _ = WAVE
    path_d = "M " + " L ".join("%.2f %.2f" % p for p in trace_points(wave))
    guides = "".join(
        '\n  <line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" stroke="#ffffff" '
        'stroke-opacity="0.42" stroke-width="2"/>'
        % (INSET, MARK / 2 - s * AMP, MARK - INSET, MARK / 2 - s * AMP) for s in (1, -1))
    with open(path, "w") as fh:
        fh.write(
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{MARK:.0f}" height="{MARK:.0f}" '
            f'viewBox="0 0 {MARK:.0f} {MARK:.0f}" role="img" aria-label="persistex">\n'
            f'  <title>persistex</title>\n'
            f'  <rect width="{MARK:.0f}" height="{MARK:.0f}" rx="30" fill="{BLUE}"/>{guides}\n'
            f'  <path d="{path_d}" fill="none" stroke="#ffffff" stroke-width="{STROKE}" '
            f'stroke-linecap="round" stroke-linejoin="round"/>\n'
            f'</svg>\n')


def write_png(path, title_fill, tag_fill, background=(255, 255, 255, 0)):
    from PIL import Image, ImageDraw, ImageFont
    scale = 4
    width = canvas_width()
    img = Image.new("RGBA", (width * scale, int(HEIGHT) * scale), background)
    dr = ImageDraw.Draw(img)
    wave, _ = WAVE
    ox, oy = PAD_X * scale, (HEIGHT - MARK) / 2 * scale
    m, inset, amp = MARK * scale, INSET * scale, AMP * scale
    dr.rounded_rectangle([ox, oy, ox + m, oy + m], radius=30 * scale, fill=(47, 111, 208, 255))
    for s in (1, -1):
        y = oy + m / 2 - s * amp
        dr.line([ox + inset, y, ox + m - inset, y], fill=(255, 255, 255, 107), width=2 * scale)
    pts = [(ox + inset + (m - 2 * inset) * i / (len(wave) - 1), oy + m / 2 - wave[i] * amp)
           for i in range(len(wave))]
    dr.line(pts, fill=(255, 255, 255, 255), width=int(STROKE * scale), joint="curve")
    try:
        big = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial Bold.ttf",
                                 int(TITLE_SIZE * scale))
        small = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf",
                                   int(TAG_SIZE * scale))
    except OSError:
        big = small = ImageFont.load_default()
    text_x = (PAD_X + MARK + GAP) * scale
    dr.text((text_x, (HEIGHT / 2 + 2) * scale), "persistex", font=big, fill=title_fill, anchor="ls")
    dr.text((text_x + 2 * scale, (HEIGHT / 2 + 30) * scale), TAGLINE, font=small,
            fill=tag_fill, anchor="ls")
    img.resize((width, int(HEIGHT)), Image.LANCZOS).save(path)


WAVE = waveform()

if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    out = lambda name: os.path.join(here, name)
    write_svg(out("logo-light.svg"), "#25303f", "#7b8899")
    write_svg(out("logo-dark.svg"), "#e8edf3", "#94a3b4")
    write_mark(out("mark.svg"))
    try:
        write_png(out("logo.png"), (37, 48, 63, 255), (123, 136, 153, 255))
        write_png(out("logo-dark.png"), (232, 237, 243, 255), (148, 163, 180, 255))
    except ImportError:
        print("Pillow not installed; SVG only")
    print("logo width %dpx, waveform RPF %.3f" % (canvas_width(), WAVE[1]))
