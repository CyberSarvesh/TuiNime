"""Render the recorded demo frames into the README's GIF and screenshots.

    cargo test record_demo -- --ignored     # records target/demo/frames.json (needs network)
    python docs/tools/render_demo.py        # writes docs/media/demo.gif and docs/media/*.png

Each frame is the app's real screen buffer (characters + colors) as ratatui drew it.
Box drawing, block elements and braille are drawn as shapes so borders join seamlessly;
text uses Cascadia Mono with symbol / Thai / Japanese fallbacks (Windows fonts).
Needs Pillow and ffmpeg on PATH.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unicodedata
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
FRAMES = ROOT / "target" / "demo" / "frames.json"
OUT = ROOT / "docs" / "media"
FONTS = Path(os.environ.get("WINDIR", "C:/Windows")) / "Fonts"

SIZE = 17                 # font px
CW, CH = 10, 21           # cell size px
PAD, BAR, RADIUS = 18, 36, 14
BACKDROP = (13, 12, 11)
DEFAULT_FG, DEFAULT_BG = "#d5c4a1", "#1a1816"


def font(name, size=SIZE, index=0):
    return ImageFont.truetype(str(FONTS / name), size, index=index)


MONO = font("CascadiaMono.ttf")
MONO_BOLD = ImageFont.truetype(str(FONTS / "CascadiaMono.ttf"), SIZE)
try:
    MONO_BOLD.set_variation_by_name("Bold")
except Exception:  # static font: fake bold by drawing twice
    MONO_BOLD = None
SYMBOL = font("seguisym.ttf", SIZE - 1)
THAI = font("LeelawUI.ttf", SIZE)
CJK = font("YuGothM.ttc", SIZE)
SYMBOLS = set("★⏎⌕↺▶▼●↔←↑→↓›‹▲✗")


def rgb(hexcolor, default):
    h = (hexcolor or default).lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def font_for(ch):
    if ch in SYMBOLS:
        return SYMBOL
    if "\u0e00" <= ch <= "\u0e7f":
        return THAI
    if unicodedata.east_asian_width(ch) in "WF" or ch == "ー":
        return CJK
    return MONO


def is_wide(ch):
    return len(ch) == 1 and unicodedata.east_asian_width(ch) in "WF"


# ------------------------------------------------------------------ shape glyphs

def draw_box(d, ch, x, y, fg):
    """Box-drawing characters as lines through the cell center (rounded corners as arcs)."""
    cx, cy = x + CW // 2, y + CH // 2
    w = 1
    if ch == "─":
        d.line([(x, cy), (x + CW, cy)], fill=fg, width=w)
    elif ch == "━":
        d.rectangle([x, cy - 1, x + CW - 1, cy + 1], fill=fg)
    elif ch == "│":
        d.line([(cx, y), (cx, y + CH)], fill=fg, width=w)
    else:
        r = CW // 2
        # (arc box, start, end, straight segment)
        spec = {
            "╭": ((cx, cy, cx + 2 * r, cy + 2 * r), 180, 270, [(cx + r, cy, x + CW, cy), (cx, cy + r, cx, y + CH)]),
            "╮": ((cx - 2 * r, cy, cx, cy + 2 * r), 270, 360, [(x, cy, cx - r, cy), (cx, cy + r, cx, y + CH)]),
            "╰": ((cx, cy - 2 * r, cx + 2 * r, cy), 90, 180, [(cx + r, cy, x + CW, cy), (cx, y, cx, cy - r)]),
            "╯": ((cx - 2 * r, cy - 2 * r, cx, cy), 0, 90, [(x, cy, cx - r, cy), (cx, y, cx, cy - r)]),
        }[ch]
        d.arc(spec[0], spec[1], spec[2], fill=fg, width=w)
        for seg in spec[3]:
            d.line([(seg[0], seg[1]), (seg[2], seg[3])], fill=fg, width=w)


def draw_block(d, ch, x, y, fg, bg):
    if ch == "▀":
        d.rectangle([x, y, x + CW - 1, y + CH // 2 - 1], fill=fg)
        d.rectangle([x, y + CH // 2, x + CW - 1, y + CH - 1], fill=bg)
    elif ch == "█":
        d.rectangle([x, y, x + CW - 1, y + CH - 1], fill=fg)
    elif ch == "▌":
        d.rectangle([x, y, x + CW // 2 - 1, y + CH - 1], fill=fg)
    elif ch == "░":
        mix = tuple((3 * b + f) // 4 for f, b in zip(fg, bg))
        d.rectangle([x + 1, y + CH // 2 - 2, x + CW - 2, y + CH // 2 + 1], fill=mix)


def draw_braille(d, ch, x, y, fg):
    bits = ord(ch) - 0x2800
    dots = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (0, 3), (1, 3)]
    for i, (col, row) in enumerate(dots):
        if bits & (1 << i):
            px, py = x + 3 + col * 4, y + 4 + row * 4
            d.ellipse([px - 1, py - 1, px + 1, py + 1], fill=fg)


# ------------------------------------------------------------------ frames

def render_screen(frame, width, height):
    img = Image.new("RGB", (width * CW, height * CH), rgb(None, DEFAULT_BG))
    d = ImageDraw.Draw(img)
    rows = frame["cells"]
    for yy, row in enumerate(rows):  # backgrounds first so wide glyphs aren't covered
        for xx, (ch, fg, bg, bold) in enumerate(row):
            d.rectangle([xx * CW, yy * CH, xx * CW + CW - 1, yy * CH + CH - 1], fill=rgb(bg, DEFAULT_BG))
    for yy, row in enumerate(rows):
        skip = False
        for xx, (ch, fg, bg, bold) in enumerate(row):
            if skip:
                skip = False
                continue
            if not ch or ch == " ":
                continue
            x, y = xx * CW, yy * CH
            f, b = rgb(fg, DEFAULT_FG), rgb(bg, DEFAULT_BG)
            if ch in "─━│╭╮╰╯":
                draw_box(d, ch, x, y, f)
            elif ch in "▀█▌░":
                draw_block(d, ch, x, y, f, b)
            elif "\u2800" <= ch <= "\u28ff":
                draw_braille(d, ch, x, y, f)
            else:
                fnt = font_for(ch)
                if bold and MONO_BOLD and fnt is MONO:
                    fnt = MONO_BOLD
                d.text((x, y + CH - 4), ch, font=fnt, fill=f, anchor="ls")
                if bold and fnt is MONO and not MONO_BOLD:
                    d.text((x + 1, y + CH - 4), ch, font=fnt, fill=f, anchor="ls")
                skip = is_wide(ch)
    return img


def window(screen):
    """Wrap a screen in a terminal window with a title bar."""
    w, h = screen.size
    win_w, win_h = w + 2, h + BAR + 1
    canvas = Image.new("RGB", (win_w + 2 * PAD, win_h + 2 * PAD), BACKDROP)
    d = ImageDraw.Draw(canvas)
    x0, y0 = PAD, PAD
    d.rounded_rectangle([x0, y0, x0 + win_w, y0 + win_h], RADIUS, fill=(38, 34, 30), outline=(70, 62, 52))
    for i, color in enumerate([(237, 106, 94), (245, 191, 79), (98, 197, 84)]):
        cx = x0 + 22 + i * 22
        d.ellipse([cx - 6, y0 + BAR // 2 - 6, cx + 6, y0 + BAR // 2 + 6], fill=color)
    title = "tuinime — anime in your terminal"
    d.text((x0 + win_w // 2, y0 + BAR // 2), title, font=font("CascadiaMono.ttf", 14), fill=(168, 153, 132), anchor="mm")
    canvas.paste(screen, (x0 + 1, y0 + BAR))
    return canvas


def banner():
    """README header: the name in the cozy theme's colors over a faint half-block pattern."""
    w, h = 1280, 300
    img = Image.new("RGB", (w, h), (26, 24, 22))
    d = ImageDraw.Draw(img)
    # faint pixel field fading in from the right, like a poster drawn with half-blocks
    palette = [(214, 167, 98), (142, 192, 124), (138, 180, 255), (226, 112, 96), (168, 153, 132)]
    for gx in range(0, w, 16):
        for gy in range(0, h, 16):
            k = ((gx * 73856093) ^ (gy * 19349663)) % 101  # scattered, but the same every run
            if k < 22 and gx > w * 0.45:
                fade = (gx - w * 0.45) / (w * 0.55)
                c = palette[k % len(palette)]
                c = tuple(int(26 + (v - 26) * 0.35 * fade) for v in c)
                d.rectangle([gx, gy, gx + 13, gy + 6], fill=c)
                d.rectangle([gx, gy + 7, gx + 13, gy + 13], fill=tuple(max(0, v - 10) for v in c))
    d.rounded_rectangle([1, 1, w - 2, h - 2], 18, outline=(92, 80, 65), width=2)
    title = ImageFont.truetype(str(FONTS / "CascadiaMono.ttf"), 112)
    try:
        title.set_variation_by_name("Bold")
    except Exception:
        pass
    d.text((86, 78), "ฅ", font=font("LeelawUI.ttf", 96), fill=(214, 167, 98))
    d.text((176, 70), "tuinime", font=title, fill=(214, 167, 98))
    d.text((182, 200), "anime, in your terminal", font=font("CascadiaMono.ttf", 34), fill=(213, 196, 161))
    for i, (name, color) in enumerate([("cozy", (214, 167, 98)), ("midnight", (138, 180, 255)), ("paper", (242, 236, 224))]):
        x = 186 + i * 150
        d.ellipse([x, 256, x + 12, 268], fill=color)
        d.text((x + 20, 262), name, font=font("CascadiaMono.ttf", 18), fill=(124, 111, 100), anchor="lm")
    OUT.mkdir(parents=True, exist_ok=True)
    img.save(OUT / "banner.png", optimize=True)
    print("banner:", OUT / "banner.png")


def main():
    banner()
    if not FRAMES.exists():
        sys.exit(f"no frames at {FRAMES}; run: cargo test record_demo -- --ignored")
    data = json.loads(FRAMES.read_text(encoding="utf-8"))
    OUT.mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix="tuinime-demo-"))
    concat = []
    try:
        for i, frame in enumerate(data["frames"]):
            img = window(render_screen(frame, data["width"], data["height"]))
            path = tmp / f"{i:04}.png"
            img.save(path)
            if frame.get("still"):
                img.save(OUT / f"{frame['still']}.png", optimize=True)
                print("still:", frame["still"])
            if frame["ms"] > 1:
                concat.append(f"file '{path.as_posix()}'\nduration {frame['ms'] / 1000:.3f}")
        concat.append(f"file '{(tmp / f'{len(data['frames']) - 1:04}.png').as_posix()}'")  # ffmpeg needs the last file twice
        (tmp / "list.txt").write_text("\n".join(concat), encoding="utf-8")
        gif = OUT / "demo.gif"
        subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(tmp / "list.txt"),
             "-vf", "scale=iw*0.8:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192:stats_mode=full[p];[b][p]paletteuse=dither=sierra2_4a",
             "-loop", "0", str(gif)],
            check=True,
        )
        print(f"gif: {gif} ({gif.stat().st_size / 1e6:.1f} MB)")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
