#!/usr/bin/env bash
# Génère assets/AppIcon.icns à partir de assets/icon.svg.
# Rendu via NSImage (script Swift) + iconutil. Le .icns généré est versionné,
# donc le build du bundle ne dépend pas de ces outils.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SVG="assets/icon.svg"
SET="assets/AppIcon.iconset"
ICNS="assets/AppIcon.icns"

rm -rf "$SET"
mkdir -p "$SET"

render() { # taille fichier
  swift scripts/render_svg.swift "$SVG" "$1" "$SET/$2"
}

render 16   icon_16x16.png
render 32   icon_16x16@2x.png
render 32   icon_32x32.png
render 64   icon_32x32@2x.png
render 128  icon_128x128.png
render 256  icon_128x128@2x.png
render 256  icon_256x256.png
render 512  icon_256x256@2x.png
render 512  icon_512x512.png
render 1024 icon_512x512@2x.png

iconutil -c icns "$SET" -o "$ICNS"
rm -rf "$SET"
echo "[make_icon] généré : $ICNS"
