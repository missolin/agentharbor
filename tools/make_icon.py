#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 SkillHub 应用图标（纯标准库，可复现）。

为什么不用 AI 画：图标是几何图形，用有符号距离场（SDF）自己算覆盖率，
边缘比生成的位图干净、颜色是精确的，而且改一行就能重画。

输出 1024x1024 RGBA PNG 主图，再交给 sips + iconutil 生成 .icns。

用法：
  /usr/bin/python3 tools/make_icon.py            # 只出主图
  /usr/bin/python3 tools/make_icon.py --icns     # 顺手把 icons/ 里整套做出来
"""

import math
import os
import struct
import subprocess
import sys
import zlib

SIZE = 1024
BG = (11, 110, 110)          # 与界面强调色一致 #0B6E6E
FG = (255, 255, 255)
SPOKE_ALPHA = 0.46
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
ICONS = os.path.join(ROOT, "src-tauri", "icons")


# ---------------------------------------------------------------- 距离场
def sdf_round_rect(px, py, cx, cy, hw, hh, r):
    dx = abs(px - cx) - hw + r
    dy = abs(py - cy) - hh + r
    outside = math.hypot(max(dx, 0.0), max(dy, 0.0))
    inside = min(max(dx, dy), 0.0)
    return outside + inside - r


def sdf_circle(px, py, cx, cy, r):
    return math.hypot(px - cx, py - cy) - r


def sdf_segment(px, py, ax, ay, bx, by, r):
    vx, vy = bx - ax, by - ay
    wx, wy = px - ax, py - ay
    L2 = vx * vx + vy * vy
    t = 0.0 if L2 == 0 else max(0.0, min(1.0, (wx * vx + wy * vy) / L2))
    return math.hypot(wx - vx * t, wy - vy * t) - r


def cover(d):
    """把距离转成覆盖率（1px 过渡带 = 抗锯齿）。"""
    c = 0.5 - d
    if c <= 0.0:
        return 0.0
    if c >= 1.0:
        return 1.0
    return c


# ---------------------------------------------------------------- 构图
def build():
    cx = cy = SIZE / 2.0
    inset = 52.0
    radius = 214.0
    hub_r = 98.0
    sat_r = 62.0
    orbit = 292.0
    spoke_w = 27.0
    n_sat = 6

    sats = []
    for i in range(n_sat):
        a = -math.pi / 2 + i * 2 * math.pi / n_sat
        sats.append((cx + orbit * math.cos(a), cy + orbit * math.sin(a)))

    rows = []
    for y in range(SIZE):
        py = y + 0.5
        row = bytearray()
        for x in range(SIZE):
            px = x + 0.5

            # 底：圆角方
            bg = cover(sdf_round_rect(px, py, cx, cy,
                                      SIZE / 2 - inset, SIZE / 2 - inset, radius))
            if bg <= 0.0:
                row += b"\x00\x00\x00\x00"
                continue

            r, g, b = BG
            a = bg

            # 轮辐（半透明白）
            sp = 0.0
            for sx, sy in sats:
                d = sdf_segment(px, py, cx, cy, sx, sy, spoke_w)
                c = cover(d)
                if c > sp:
                    sp = c
            if sp > 0.0:
                sa = sp * SPOKE_ALPHA
                r = r * (1 - sa) + FG[0] * sa
                g = g * (1 - sa) + FG[1] * sa
                b = b * (1 - sa) + FG[2] * sa
                a = a * (1 - sa) + sa

            # 中心 + 外围圆点（实心白）
            dots = cover(sdf_circle(px, py, cx, cy, hub_r))
            for sx, sy in sats:
                c = cover(sdf_circle(px, py, sx, sy, sat_r))
                if c > dots:
                    dots = c
            if dots > 0.0:
                r = r * (1 - dots) + FG[0] * dots
                g = g * (1 - dots) + FG[1] * dots
                b = b * (1 - dots) + FG[2] * dots
                a = a * (1 - dots) + dots

            row += bytes((int(round(r)), int(round(g)), int(round(b)),
                          int(round(a * 255))))
        rows.append(bytes(row))
    return rows


def write_png(path, rows):
    raw = b"".join(b"\x00" + r for r in rows)          # 每行前面一个 filter 字节

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def make_icns(master):
    os.makedirs(ICONS, exist_ok=True)
    iset = os.path.join(ICONS, "AppIcon.iconset")
    os.makedirs(iset, exist_ok=True)
    specs = [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2),
             (256, 1), (256, 2), (512, 1), (512, 2)]
    for base, scale in specs:
        px = base * scale
        name = "icon_%dx%d%s.png" % (base, base, "@2x" if scale == 2 else "")
        subprocess.run(["sips", "-z", str(px), str(px), master, "--out",
                        os.path.join(iset, name)],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    subprocess.run(["iconutil", "-c", "icns", iset, "-o",
                    os.path.join(ICONS, "icon.icns")], check=True)
    # tauri.conf.json 里点名要的两个尺寸
    subprocess.run(["sips", "-z", "32", "32", master, "--out",
                    os.path.join(ICONS, "32x32.png")],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    subprocess.run(["sips", "-z", "128", "128", master, "--out",
                    os.path.join(ICONS, "128x128.png")],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    subprocess.run(["sips", "-z", "256", "256", master, "--out",
                    os.path.join(ICONS, "128x128@2x.png")],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    subprocess.run(["sips", "-z", "512", "512", master, "--out",
                    os.path.join(ICONS, "icon.png")],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    import shutil
    shutil.rmtree(iset, ignore_errors=True)


def main():
    master = os.path.join(ICONS, "source-1024.png")
    os.makedirs(ICONS, exist_ok=True)
    print("正在算 %dx%d 的像素…" % (SIZE, SIZE))
    write_png(master, build())
    print("主图:", master, "%.0f KB" % (os.path.getsize(master) / 1024))
    if "--icns" in sys.argv:
        make_icns(master)
        print("已生成:", os.path.join(ICONS, "icon.icns"))
        for f in sorted(os.listdir(ICONS)):
            print("   ", f)
    return 0


if __name__ == "__main__":
    sys.exit(main())
