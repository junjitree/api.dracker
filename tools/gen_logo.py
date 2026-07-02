#!/usr/bin/env python3
"""Fill the logo SVG template with a REAL, scannable QR code.

Reads logo.template.svg ({{QR_MODULES}} placeholder), encodes PAYLOAD with
qrencode (includes a 4-module quiet zone), then renders:
  - proper concentric finder patterns (1-module ring + 3-module center)
  - near-contiguous rounded data modules
so the result still reads as the logo but scans as a valid QR.
"""
import subprocess, pathlib

PAYLOAD = "Hi!"
QUIET   = 2                 # quiet-zone modules each side (tightened)
CX, CY  = 256, 268         # QR centre in the 512 canvas
SIZE    = 214              # QR bounding box (incl. quiet zone), px

HERE = pathlib.Path(__file__).resolve().parent
TEMPLATE = HERE / "logo.template.svg"
OUT      = HERE.parent / "logo.svg"


def matrix(payload):
    txt = subprocess.check_output(
        ["qrencode", "-t", "ASCII", "-m", str(QUIET), payload]
    ).decode().splitlines()
    return [[row[2 * c:2 * c + 2] == "##" for c in range(len(row) // 2)] for row in txt]


def main():
    m = matrix(PAYLOAD)
    N = len(m)
    cell = SIZE / N
    x0 = CX - SIZE / 2
    y0 = CY - SIZE / 2

    def px(c): return x0 + c * cell
    def py(r): return y0 + r * cell

    # finder top-left corners (module coords), given quiet zone QUIET
    finders = [(QUIET, QUIET), (QUIET, N - QUIET - 7), (N - QUIET - 7, QUIET)]

    def in_finder(r, c):
        return any(fr <= r < fr + 7 and fc <= c < fc + 7 for fr, fc in finders)

    parts = []

    # data + timing modules as near-contiguous rounded squares
    g = cell * 0.06                       # tiny gap
    s = cell - 2 * g
    for r in range(N):
        for c in range(N):
            if m[r][c] and not in_finder(r, c):
                parts.append(
                    f'      <rect x="{px(c)+g:.2f}" y="{py(r)+g:.2f}" '
                    f'width="{s:.2f}" height="{s:.2f}" rx="{cell*0.28:.2f}"/>'
                )

    # concentric finder patterns (valid): 1-module ring + 3-module centre
    for fr, fc in finders:
        # ring: stroke width = 1 module, path centred on module lines 0.5 .. 6.5
        rx0, ry0 = px(fc + 0.5), py(fr + 0.5)
        parts.append(
            f'      <rect x="{rx0:.2f}" y="{ry0:.2f}" '
            f'width="{6*cell:.2f}" height="{6*cell:.2f}" rx="{cell*1.6:.2f}" '
            f'fill="none" stroke="url(#stroke)" stroke-width="{cell:.2f}"/>'
        )
        # centre 3x3
        parts.append(
            f'      <rect x="{px(fc+2):.2f}" y="{py(fr+2):.2f}" '
            f'width="{3*cell:.2f}" height="{3*cell:.2f}" rx="{cell*0.9:.2f}"/>'
        )

    svg = TEMPLATE.read_text().replace("{{QR_MODULES}}", "\n".join(parts))
    OUT.write_text(svg)
    print(f"payload={PAYLOAD!r}  N={N} (quiet={QUIET})  cell={cell:.2f}px  rects={len(parts)}")


if __name__ == "__main__":
    main()
