"""Génère les images du logo FlashGet (icônes de l'application, de l'extension, du site).

Le logo est fait uniquement de polygones et de rectangles arrondis : on le dessine avec
Pillow en sur-échantillonnage (x4) puis on réduit, ce qui donne des bords lissés.
Usage : python scripts/brand/render_icons.py
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
AMBER = (246, 172, 51, 255)
TILE_A = (22, 32, 58)
TILE_B = (10, 15, 30)

# Tracé de la marque, dans son repère d'origine.
F_PATH = [(795, 466), (1566, 466), (1545, 644), (965, 644), (943, 833), (1418, 833), (1397, 1013),
          (922, 1013), (894, 1252), (1027, 1252), (790, 1524), (571, 1252), (702, 1252)]
BARS_FULL = [(592, 466, 300, 75), (463, 601, 420, 75), (587, 751, 290, 75), (270, 751, 258, 75),
             (644, 901, 230, 75), (443, 901, 158, 75)]
BARS_SMALL = [(560, 466, 330, 90), (440, 740, 440, 90)]
VIEW_FULL = (250, 446, 1336, 1098)
VIEW_SMALL = (400, 446, 1186, 1098)


def draw_mark(draw, view, bars, box, k):
    """Dessine la marque dans `box` (x, y, w, h, en unités 512), centrée, à l'échelle k."""
    vx, vy, vw, vh = view
    bx, by, bw, bh = box
    s = min(bw / vw, bh / vh)
    ox = bx + (bw - vw * s) / 2
    oy = by + (bh - vh * s) / 2

    def p(x, y):
        return ((ox + (x - vx) * s) * k, (oy + (y - vy) * s) * k)

    draw.polygon([p(x, y) for x, y in F_PATH], fill=AMBER)
    for x, y, w, h in bars:
        x0, y0 = p(x, y)
        x1, y1 = p(x + w, y + h)
        draw.rounded_rectangle((x0, y0, x1, y1), radius=(y1 - y0) / 2, fill=AMBER)


def tile(size, k):
    """Tuile bleu nuit aux coins arrondis, dégradé en diagonale, liseré clair."""
    big = size * k
    grad = Image.new("RGBA", (big, big))
    gd = ImageDraw.Draw(grad)
    for i in range(2 * big):
        t = i / (2 * big - 1)
        c = tuple(round(a + (b - a) * t) for a, b in zip(TILE_A, TILE_B)) + (255,)
        gd.line([(i, 0), (0, i)], fill=c)
    mask = Image.new("L", (big, big), 0)
    u = big / 512
    ImageDraw.Draw(mask).rounded_rectangle((16 * u, 16 * u, 496 * u, 496 * u), radius=112 * u, fill=255)
    out = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    out.paste(grad, (0, 0), mask)
    # Le liseré est fusionné par-dessus (ImageDraw remplacerait les pixels au lieu de les mélanger).
    rim = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    ImageDraw.Draw(rim).rounded_rectangle((17.5 * u, 17.5 * u, 494.5 * u, 494.5 * u), radius=110.5 * u,
                                          outline=(255, 255, 255, 18), width=max(1, round(3 * u)))
    return Image.alpha_composite(out, rim)


def icon(size, small=False):
    k = 4
    img = tile(size, k)
    d = ImageDraw.Draw(img)
    u = size / 512
    if small:
        draw_mark(d, VIEW_SMALL, BARS_SMALL, (66 * u, 76 * u, 380 * u, 360 * u), k)
    else:
        draw_mark(d, VIEW_FULL, BARS_FULL, (76 * u, 96 * u, 360 * u, 320 * u), k)
    return img.resize((size, size), Image.LANCZOS)


def mark(width):
    """Marque seule sur fond transparent, à la largeur demandée."""
    vx, vy, vw, vh = VIEW_FULL
    h = round(width * vh / vw)
    k = 4
    img = Image.new("RGBA", (width * k, h * k), (0, 0, 0, 0))
    draw_mark(ImageDraw.Draw(img), VIEW_FULL, BARS_FULL, (0, 0, width, h), k)
    return img.resize((width, h), Image.LANCZOS)


def main():
    brand = ROOT / "docs" / "brand"
    brand.mkdir(parents=True, exist_ok=True)
    icon(1024).save(brand / "flashget-icon-1024.png")
    mark(1024).save(brand / "flashget-mark-1024.png")

    # Extension : 16 et 32 px en version simplifiée.
    ext = ROOT / "extension" / "icons"
    for s in (16, 32):
        icon(s, small=True).save(ext / f"{s}.png")
    for s in (48, 128):
        icon(s).save(ext / f"{s}.png")

    # Site et README.
    icon(256).save(ROOT / "docs" / "presentation" / "assets" / "flashget-256.png")
    icon(32, small=True).save(ROOT / "docs" / "presentation" / "assets" / "favicon-32.png")
    print("images générées dans docs/brand, extension/icons, docs/presentation/assets")


def finish_tauri_icons():
    """Après `tauri icon` : petites tailles en version simplifiée, .ico multi-tailles."""
    icons = ROOT / "src-tauri" / "icons"
    icon(32, small=True).save(icons / "32x32.png")
    sizes = [16, 24, 32, 48, 64, 128, 256]
    frames = [icon(s, small=s <= 32) for s in sizes]
    frames[-1].save(icons / "icon.ico", format="ICO", sizes=[(s, s) for s in sizes], append_images=frames[:-1])
    print("32x32.png et icon.ico mis à jour")


if __name__ == "__main__":
    import sys
    finish_tauri_icons() if "--finish" in sys.argv else main()
