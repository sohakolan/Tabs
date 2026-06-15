#!/usr/bin/env bash
# Crée un fichier d'installation dist/Tabs-<arch>.dmg à partir de dist/Tabs.app.
#
# Le DMG contient l'app et un raccourci vers /Applications : l'utilisateur glisse
# « Tabs » sur « Applications » pour l'installer. Pensé pour une release GitHub.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APP="dist/Tabs.app"
ARCH="$(uname -m)"            # arm64 (Apple Silicon) ou x86_64 (Intel)
VOL="Tabs"
DMG="dist/Tabs-${ARCH}.dmg"

if [[ ! -d "$APP" ]]; then
  echo "[dmg] $APP introuvable — lance « make bundle » d'abord." >&2
  exit 1
fi

echo "[dmg] préparation de l'image (${ARCH})…"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"   # cible du glisser-déposer

rm -f "$DMG"
echo "[dmg] création de ${DMG}…"
hdiutil create \
  -volname "$VOL" \
  -srcfolder "$STAGE" \
  -fs HFS+ \
  -format UDZO \
  -ov \
  "$DMG" >/dev/null

echo "[dmg] terminé : $DMG"
