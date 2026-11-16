#!/usr/bin/env python3
"""生成应用图标（零第三方依赖，仅用标准库）。

产出：
  icons/icon.png            1024x1024  打包主图标
  icons/128x128.png         128x128
  icons/128x128@2x.png      256x256
  icons/32x32.png           32x32
  icons/icon.ico            Windows 多尺寸
  icons/icon.icns           macOS 多尺寸
  icons/tray.png            托盘图标（macOS 模板图，纯黑 + 透明）
"""

import os
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src-tauri", "icons")

BRAND = (37, 99, 235, 255)     # #2563eb
BRAND_DARK = (15, 23, 42, 255)  # #0f172a
WHITE = (255, 255, 255, 255)
BLACK = (0, 0, 0, 255)


def png_bytes(width, height, rgba_rows):
    """把 RGBA 行数据编码为 PNG。"""
    raw = b"".join(b"\x00" + row for row in rgba_rows)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def rounded_square(size):
    """生成圆角方形底图 + 居中的『隧道环』图形。"""
    radius = size * 0.22          # 圆角
    ring_outer = size * 0.40
    ring_inner = size * 0.40 - size * 0.075
    cx = cy = size / 2.0

    rows = []
    for y in range(size):
        row = bytearray()
        for x in range(size):
            px, py = x + 0.5, y + 0.5
            # 圆角矩形遮罩
            dx = max(radius - px, px - (size - radius), 0)
            dy = max(radius - py, py - (size - radius), 0)
            inside = (dx * dx + dy * dy) <= radius * radius
            if not inside:
                row += bytes((0, 0, 0, 0))
                continue
            # 底色：从左上到右下的渐变
            t = (px + py) / (2 * size)
            base = tuple(
                int(BRAND_DARK[i] + (BRAND[i] - BRAND_DARK[i]) * t) for i in range(3)
            )
            # 环
            d = ((px - cx) ** 2 + (py - cy) ** 2) ** 0.5
            if ring_inner <= d <= ring_outer:
                # 环上开一个缺口，形成『隧道入口』效果
                if not (px > cx and abs(py - cy) < size * 0.055):
                    row += bytes(WHITE)
                    continue
            # 中心横线（隧道）
            if abs(py - cy) < size * 0.055 and abs(px - cx) < ring_inner * 0.62:
                row += bytes(WHITE)
                continue
            row += bytes(base) + b"\xff"
        rows.append(bytes(row))
    return rows


def ring_only(size, color):
    """托盘图标：仅环形的单色图（macOS 模板图依赖 alpha）。"""
    cx = cy = size / 2.0
    outer = size * 0.46
    inner = size * 0.46 - max(size * 0.115, 1.6)
    rows = []
    for y in range(size):
        row = bytearray()
        for x in range(size):
            px, py = x + 0.5, y + 0.5
            d = ((px - cx) ** 2 + (py - cy) ** 2) ** 0.5
            if inner <= d <= outer:
                # 缺口
                if px > cx and abs(py - cy) < size * 0.09:
                    row += b"\x00\x00\x00\x00"
                    continue
                row += bytes(color)
            elif abs(py - cy) < size * 0.09 and abs(px - cx) < inner * 0.6:
                row += bytes(color)
            else:
                row += b"\x00\x00\x00\x00"
        rows.append(bytes(row))
    return rows


def write_png(path, size, rows):
    with open(path, "wb") as f:
        f.write(png_bytes(size, size, rows))


def write_ico(path, images):
    """images: [(size, png_bytes), ...]"""
    count = len(images)
    header = struct.pack("<HHH", 0, 1, count)
    offset = 6 + 16 * count
    entries = b""
    blobs = b""
    for size, data in images:
        dim = 0 if size >= 256 else size
        entries += struct.pack(
            "<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset
        )
        blobs += data
        offset += len(data)
    with open(path, "wb") as f:
        f.write(header + entries + blobs)


def write_icns(path, images):
    """images: [(ostype, png_bytes), ...]"""
    body = b""
    for ostype, data in images:
        body += ostype + struct.pack(">I", len(data) + 8) + data
    with open(path, "wb") as f:
        f.write(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main():
    os.makedirs(OUT_DIR, exist_ok=True)

    # 主图标多尺寸
    sizes = {
        32: "32x32.png",
        64: "64x64.png",
        128: "128x128.png",
        256: "128x128@2x.png",
        512: "icon-512.png",
        1024: "icon.png",
    }
    cache = {}
    for size, name in sizes.items():
        rows = rounded_square(size)
        data = png_bytes(size, size, rows)
        cache[size] = data
        with open(os.path.join(OUT_DIR, name), "wb") as f:
            f.write(data)

    # Windows ICO
    write_ico(
        os.path.join(OUT_DIR, "icon.ico"),
        [(16, png_bytes(16, 16, rounded_square(16))),
         (32, cache[32]), (64, cache[64]), (128, cache[128]), (256, cache[256])],
    )

    # macOS ICNS
    write_icns(
        os.path.join(OUT_DIR, "icon.icns"),
        [
            (b"ic07", cache[128]),
            (b"ic08", cache[256]),
            (b"ic09", cache[512]),
            (b"ic10", cache[1024]),
            (b"ic11", cache[32]),
            (b"ic12", cache[64]),
            (b"ic13", cache[256]),
            (b"ic14", cache[512]),
        ],
    )

    # 托盘（单色，供 macOS 模板模式自动反色）
    write_png(
        os.path.join(OUT_DIR, "tray.png"),
        64,
        ring_only(64, BLACK),
    )
    write_png(
        os.path.join(OUT_DIR, "tray@2x.png"),
        128,
        ring_only(128, BLACK),
    )

    for f in sorted(os.listdir(OUT_DIR)):
        p = os.path.join(OUT_DIR, f)
        print(f"{f:20s} {os.path.getsize(p):>8d} bytes")


if __name__ == "__main__":
    main()