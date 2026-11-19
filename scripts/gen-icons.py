#!/usr/bin/env python3
"""生成 Octauri 全套图标（Aurora 方案：蓝紫渐变 + 白色开口环 + 中心点）。

依赖：Pillow（`python -m pip install pillow`）。
产物（src-tauri/icons/）：
  icon.png            1024x1024  主图标
  128x128.png         128
  128x128@2x.png      256
  32x32.png           32
  64x64.png           64
  icon-512.png        512
  icon.ico            16/24/32/48/64/128/256
  icon.icns           16/32/64/128/256/512/1024
  tray.png            macOS 菜单栏模板图（黑色 + 透明，系统自动反色）
  tray-windows.png    Windows 托盘彩色迷你图标
"""
import io
import math
import os
import struct

from PIL import Image, ImageDraw, ImageFilter, ImageChops, ImageOps

OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src-tauri", "icons")

# 品牌渐变（Aurora）
TOP = (64, 132, 250)
BOTTOM = (88, 28, 190)
WHITE = (255, 255, 255, 255)
BLACK = (0, 0, 0, 255)
TRAY_BG = (72, 80, 230, 255)

SS = 2  # 超采样倍数（主图 1024 用 2x；小尺寸临时改成 4x）


def px(v, s):
    return int(round(v * s))


# --------------------------------------------------------------------------- #
# 基础：渐变底 + 圆角遮罩
# --------------------------------------------------------------------------- #
def gradient_bg(size, top=TOP, bottom=BOTTOM):
    s = size * SS
    # 先在小图上按 x+y 逐像素生成对角渐变，再放大。
    # 不能用 rotate 裁剪：旋转后的四角是填充色，会在图标四角留下纯色块。
    n = 256
    small = Image.new("RGB", (n, n))
    dst = small.load()
    for yy in range(n):
        for xx in range(n):
            t = (xx + yy) / (2 * (n - 1))
            dst[xx, yy] = (
                int(top[0] + (bottom[0] - top[0]) * t),
                int(top[1] + (bottom[1] - top[1]) * t),
                int(top[2] + (bottom[2] - top[2]) * t),
            )
    img = small.resize((s, s), Image.BICUBIC).convert("RGBA")

    # 左上柔光，让渐变不死板
    glow = Image.new("L", (s, s), 0)
    ImageDraw.Draw(glow).ellipse(
        [px(0.02, s), px(-0.08, s), px(0.62, s), px(0.52, s)], fill=255
    )
    glow = glow.filter(ImageFilter.GaussianBlur(px(0.18, s)))
    glow = glow.point(lambda v: int(v * 0.10))
    img.alpha_composite(Image.composite(
        Image.new("RGBA", (s, s), (255, 255, 255, 255)),
        Image.new("RGBA", (s, s), (0, 0, 0, 0)),
        glow,
    ))

    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, s - 1, s - 1], radius=int(s * 0.225), fill=255
    )
    out = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)
    return out


# --------------------------------------------------------------------------- #
# 图形：开口环 + 中心点
# --------------------------------------------------------------------------- #
def glyph_mask(size, outer=0.33, inner=0.215, half=22, dot=0.076, gap_deg=-45):
    s = size * SS
    mask = Image.new("L", (s, s), 0)
    d = ImageDraw.Draw(mask)
    width = outer - inner
    rc = outer - width / 2  # PIL arc 的线宽朝 bbox 内侧展开
    a0 = gap_deg + half
    a1 = gap_deg - half + 360
    d.arc(
        [px(0.5 - outer, s), px(0.5 - outer, s),
         px(0.5 + outer, s), px(0.5 + outer, s)],
        start=a0, end=a1, fill=255, width=max(1, px(width, s)),
    )
    for ang in (a0, a1):
        r = width / 2
        cx = 0.5 + rc * math.cos(math.radians(ang))
        cy = 0.5 + rc * math.sin(math.radians(ang))
        d.ellipse([px(cx - r, s), px(cy - r, s), px(cx + r, s), px(cy + r, s)], fill=255)
    d.ellipse(
        [px(0.5 - dot, s), px(0.5 - dot, s), px(0.5 + dot, s), px(0.5 + dot, s)],
        fill=255,
    )
    return mask


def shifted(img, dx, dy):
    out = Image.new("L", img.size, 0)
    out.paste(img, (int(dx), int(dy)))
    return out


def paint_solid(size, mask, color):
    """用 mask 把纯色画到透明层上（正确处理 alpha）。"""
    s = size * SS
    layer = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    layer.paste(Image.new("RGBA", (s, s), color), (0, 0), mask)
    return layer


# --------------------------------------------------------------------------- #
# 渲染
# --------------------------------------------------------------------------- #
def render_app(size, detailed=True):
    s = size * SS
    bg = gradient_bg(size)

    if detailed:
        mask = glyph_mask(size)
        # 环境投影
        shadow = mask.filter(ImageFilter.GaussianBlur(px(0.018, s)))
        sl = paint_solid(size, shadow, (6, 16, 48, 90))
        bg.alpha_composite(sl, (px(0.006, s), px(0.012, s)))

    mask = glyph_mask(size)
    bg.alpha_composite(paint_solid(size, mask, WHITE))

    if detailed:
        # 斜面：左上高光 / 右下阴影
        dx, dy = int(0.012 * s), int(0.015 * s)
        hi = ImageChops.subtract(mask, shifted(mask, dx, dy))
        sh = ImageChops.subtract(mask, shifted(mask, -dx, -dy))
        hi = hi.filter(ImageFilter.GaussianBlur(px(0.004, s)))
        sh = sh.filter(ImageFilter.GaussianBlur(px(0.005, s)))
        bg.alpha_composite(paint_solid(size, hi, (255, 255, 255, 130)))
        bg.alpha_composite(paint_solid(size, sh, (40, 10, 90, 95)))

    return bg.resize((size, size), Image.LANCZOS)


def render_tray_windows(size=64):
    s = size * SS
    bg = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    ImageDraw.Draw(bg).rounded_rectangle(
        [0, 0, s - 1, s - 1], radius=int(s * 0.225), fill=TRAY_BG
    )
    mask = glyph_mask(size, outer=0.335, inner=0.205, half=25, dot=0.082)
    bg.alpha_composite(paint_solid(size, mask, WHITE))
    return bg.resize((size, size), Image.LANCZOS)


def render_tray_macos(size=64):
    """模板图：纯黑 + alpha，图形撑满画布（菜单栏尺寸小，需要更饱满）。"""
    s = size * SS
    mask = glyph_mask(size, outer=0.46, inner=0.345, half=25, dot=0.105)
    layer = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    layer.paste(Image.new("RGBA", (s, s), BLACK), (0, 0), mask)
    return layer.resize((size, size), Image.LANCZOS)


# --------------------------------------------------------------------------- #
# 写文件（PNG / ICO / ICNS）
# --------------------------------------------------------------------------- #
def png_bytes(img):
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return buf.getvalue()


def write_ico(path, entries):
    """entries: [(size, png_bytes), ...] — PNG 压缩条目，Vista+ 支持。"""
    count = len(entries)
    header = struct.pack("<HHH", 0, 1, count)
    offset = 6 + 16 * count
    table = b""
    blobs = b""
    for size, data in entries:
        dim = 0 if size >= 256 else size
        table += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    with open(path, "wb") as f:
        f.write(header + table + blobs)


def write_icns(path, entries):
    body = b""
    for ostype, data in entries:
        body += ostype + struct.pack(">I", len(data) + 8) + data
    with open(path, "wb") as f:
        f.write(b"icns" + struct.pack(">I", len(body) + 8) + body)


# --------------------------------------------------------------------------- #
def main():
    os.makedirs(OUT_DIR, exist_ok=True)

    # 主图标：一次渲染 1024，其余尺寸由它缩放（小尺寸单独用简化画法）
    global SS
    SS = 2
    master = render_app(1024, detailed=True)
    master.save(os.path.join(OUT_DIR, "icon.png"))
    master.resize((512, 512), Image.LANCZOS).save(os.path.join(OUT_DIR, "icon-512.png"))
    master.resize((256, 256), Image.LANCZOS).save(os.path.join(OUT_DIR, "128x128@2x.png"))
    master.resize((128, 128), Image.LANCZOS).save(os.path.join(OUT_DIR, "128x128.png"))

    SS = 4
    small = {s: render_app(s, detailed=False) for s in (16, 24, 32, 48, 64)}
    small[32].save(os.path.join(OUT_DIR, "32x32.png"))
    small[64].save(os.path.join(OUT_DIR, "64x64.png"))

    ico_entries = []
    for s in (16, 24, 32, 48, 64):
        ico_entries.append((s, png_bytes(small[s])))
    for s in (128, 256):
        ico_entries.append((s, png_bytes(master.resize((s, s), Image.LANCZOS))))
    write_ico(os.path.join(OUT_DIR, "icon.ico"), ico_entries)

    icns_entries = [
        (b"ic11", png_bytes(small[32])),
        (b"ic12", png_bytes(small[64])),
        (b"ic07", png_bytes(master.resize((128, 128), Image.LANCZOS))),
        (b"ic08", png_bytes(master.resize((256, 256), Image.LANCZOS))),
        (b"ic09", png_bytes(master.resize((512, 512), Image.LANCZOS))),
        (b"ic10", png_bytes(master)),
        (b"ic13", png_bytes(master.resize((256, 256), Image.LANCZOS))),
        (b"ic14", png_bytes(master.resize((512, 512), Image.LANCZOS))),
    ]
    write_icns(os.path.join(OUT_DIR, "icon.icns"), icns_entries)

    SS = 4
    render_tray_macos(64).save(os.path.join(OUT_DIR, "tray.png"))
    render_tray_windows(64).save(os.path.join(OUT_DIR, "tray-windows.png"))

    for f in sorted(os.listdir(OUT_DIR)):
        p = os.path.join(OUT_DIR, f)
        if os.path.isfile(p):
            print(f"{f:20s} {os.path.getsize(p):>8d} bytes")


if __name__ == "__main__":
    main()