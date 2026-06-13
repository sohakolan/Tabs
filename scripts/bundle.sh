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
cp assets/AppIcon.icns "$RES/AppIcon.icns"

# Signature ad-hoc : suffisante pour le développement local et pour que macOS
# garde une identité stable des permissions (Accessibilité, Enregistrement de
# l'écran) entre deux lancements.
codesign --force --sign - "$APP" >/dev/null 2>&1 || \
	echo "[bundle] codesign indisponible, bundle non signé (ok en dev)."

echo "[bundle] terminé : $APP"
