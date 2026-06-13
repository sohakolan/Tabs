#!/usr/bin/env bash
# Rend les aperçus de modes (SVG → PNG @2x) dans assets/. PNG versionnés.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for name in thumbnails appicons titles; do
  swift scripts/render_svg.swift "assets/preview_${name}.svg" 520 "assets/preview_${name}.png" 300
done
echo "[make_previews] aperçus régénérés (520x300)."
