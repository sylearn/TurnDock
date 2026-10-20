"""生成应用图标源文件与菜单栏模板图标（纯 Python，无第三方依赖）。

图形：石色圆角方块上，一枚实心墨子与一枚空心子斜向相对，表示“轮到你”。
"""
import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
SS = 4  # 超采样倍数


def png(path, w, h, pixels):
    raw = b"".join(b"\x00" + bytes(pixels[y * w * 4:(y + 1) * w * 4]) for y in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    path.write_bytes(data)


def hexrgb(h):
    return tuple(int(h[i:i + 2], 16) for i in (1, 3, 5))


def render(size, shapes):
    """shapes: 每个像素点返回 (r,g,b,a) 或 None 的函数列表，后者覆盖前者。"""
    px = bytearray(size * size * 4)
    for y in range(size):
        for x in range(size):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sy in range(SS):
                for sx in range(SS):
                    fx = (x + (sx + 0.5) / SS) / size
                    fy = (y + (sy + 0.5) / SS) / size
                    col = None
                    for s in shapes:
                        c = s(fx, fy)
                        if c is not None:
                            col = c
                    if col:
                        a = col[3] / 255
                        for i in range(3):
                            acc[i] += col[i] * a
                        acc[3] += a
            n = SS * SS
            a = acc[3] / n
            o = (y * size + x) * 4
            if a > 0:
                for i in range(3):
                    px[o + i] = round(acc[i] / acc[3])
            px[o + 3] = round(a * 255)
    return px


def rounded_rect(x0, y0, x1, y1, r, color):
    def f(x, y):
        cx = min(max(x, x0 + r), x1 - r)
        cy = min(max(y, y0 + r), y1 - r)
        if x0 <= x <= x1 and y0 <= y <= y1 and math.hypot(x - cx, y - cy) <= r:
            return color
        return None
    return f


def disc(cx, cy, r, color):
    return lambda x, y: color if math.hypot(x - cx, y - cy) <= r else None


def ring(cx, cy, r, w, color):
    return lambda x, y: color if r - w <= math.hypot(x - cx, y - cy) <= r else None


stone = (*hexrgb("#ECEEEA"), 255)
ink = (*hexrgb("#16191A"), 255)
signal = (*hexrgb("#0E6A60"), 255)

OUT.mkdir(parents=True, exist_ok=True)
size = 1024
app = [
    rounded_rect(0.098, 0.098, 0.902, 0.902, 0.18, stone),
    ring(0.615, 0.385, 0.185, 0.045, signal),
    disc(0.385, 0.615, 0.185, ink),
]
png(OUT / "source.png", size, size, render(size, app))

# 菜单栏模板图标：黑色 + 透明，系统自动适配亮暗。
for name, s in (("tray.png", 22), ("tray@2x.png", 44)):
    tray = [ring(0.66, 0.34, 0.30, 0.10, (0, 0, 0, 255)), disc(0.36, 0.64, 0.30, (0, 0, 0, 255))]
    png(OUT / name, s, s, render(s, tray))
print("icons written to", OUT)
