"""Générateur déterministe de l'icône FlashGet (variante B : flèche + barre, dégradé ambre).

Produit :
  - assets/icon/flashget-1024.png        (master 1024x1024, RGBA, coins transparents)
  - extension/icons/{16,32,48,128}.png   (icône de l'extension Chromium)

Lancer : python assets/icon/make_icon.py
Régénère l'icône à l'identique (aucune source externe).
"""

import os
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ASSET_DIR = os.path.join(ROOT, "assets", "icon")
EXT_DIR = os.path.join(ROOT, "extension", "icons")

SS = 2                       # supersampling pour l'anticrénelage
S = 1024 * SS
TOP = (251, 191, 36)         # amber-400 #fbbf24
BOT = (245, 158, 11)         # amber-500 #f59e0b
WHITE = (255, 255, 255, 255)


def _gradient_tile():
    grad = Image.new("RGB", (S, S))
    d = ImageDraw.Draw(grad)
    for y in range(S):
        t = y / (S - 1)
        col = tuple(int(TOP[i] + (BOT[i] - TOP[i]) * t) for i in range(3))
        d.line([(0, y), (S, y)], fill=col)
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1], radius=int(S * 0.22), fill=255)
    tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    tile.paste(grad, (0, 0), mask)
    return tile


def _glyph_layer():
    ov = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(ov)
    cx = S / 2
    # flèche : tige + tête
    stem_w = 0.135 * S
    d.rounded_rectangle(
        [cx - stem_w / 2, 0.24 * S, cx + stem_w / 2, 0.52 * S],
        radius=stem_w / 2, fill=WHITE,
    )
    head_half = 0.215 * S
    d.polygon([(cx - head_half, 0.47 * S), (cx + head_half, 0.47 * S), (cx, 0.74 * S)], fill=WHITE)
    # barre de base (download-to-bar)
    bar_w = 0.44 * S
    bar_h = 0.075 * S
    by = 0.82 * S
    d.rounded_rectangle([cx - bar_w / 2, by, cx + bar_w / 2, by + bar_h], radius=bar_h / 2, fill=WHITE)
    return ov


def build_master():
    img = Image.alpha_composite(_gradient_tile(), _glyph_layer())
    return img.resize((1024, 1024), Image.LANCZOS)


def main():
    os.makedirs(ASSET_DIR, exist_ok=True)
    os.makedirs(EXT_DIR, exist_ok=True)
    master = build_master()
    master.save(os.path.join(ASSET_DIR, "flashget-1024.png"))
    for size in (16, 32, 48, 128):
        master.resize((size, size), Image.LANCZOS).save(os.path.join(EXT_DIR, f"{size}.png"))
    print("icône générée : assets/icon/flashget-1024.png + extension/icons/{16,32,48,128}.png")


if __name__ == "__main__":
    main()
