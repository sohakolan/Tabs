#!/usr/bin/env bash
# Assemble dist/Tabs.app à partir du binaire release et de Info.plist.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APP="dist/Tabs.app"
MACOS="$APP/Contents/MacOS"
RES="$APP/Contents/Resources"

echo "[bundle] compilation release…"
cargo build --release

echo "[bundle] assemblage de ${APP}…"
rm -rf "$APP"
mkdir -p "$MACOS" "$RES"
cp target/release/tabs "$MACOS/tabs"
cp Info.plist "$APP/Contents/Info.plist"

# Synchronise la version du bundle avec celle de Cargo.toml (source unique de
# vérité) AVANT la signature, pour que l'app ne se déclare jamais à une ancienne
# version dans son Info.plist (sinon « brew upgrade » semble ne rien faire).
VERSION="$(grep -m1 -E '^version[[:space:]]*=' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
if [[ -n "$VERSION" ]]; then
	/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist"
	/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"
	echo "[bundle] version du bundle : $VERSION"
fi

cp assets/AppIcon.icns "$RES/AppIcon.icns"
cp assets/preview_thumbnails.png assets/preview_appicons.png assets/preview_titles.png "$RES/"

# Signature : on privilégie une identité auto-signée STABLE (« Tabs Dev », créée
# par scripts/setup_signing.sh) pour que les permissions (Accessibilité,
# Enregistrement de l'écran) persistent entre les rebuilds. À défaut, signature
# ad-hoc (les permissions seront alors redemandées après chaque build).
# On accepte l'identité même NON approuvée (« CSSMERR_TP_NOT_TRUSTED ») : un
# cert auto-signé suffit à `codesign`, et c'est l'unique condition pour que TCC
# garde les permissions (il lui faut une identité STABLE, pas approuvée). D'où
# `find-identity -p codesigning` SANS `-v` (qui, lui, masque les non approuvées).
IDENTITY="Tabs Dev"
if security find-identity -p codesigning 2>/dev/null | grep -q "$IDENTITY" \
	&& codesign --force --sign "$IDENTITY" "$APP" >/dev/null 2>&1; then
	echo "[bundle] signé avec l'identité stable « $IDENTITY » (permissions persistantes)."
elif codesign --force --sign - "$APP" >/dev/null 2>&1; then
	echo "[bundle] signé en ad-hoc (lance scripts/setup_signing.sh pour des permissions persistantes)."
else
	echo "[bundle] codesign indisponible, bundle non signé."
fi

echo "[bundle] terminé : $APP"
