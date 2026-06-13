#!/usr/bin/env bash
# Crée une identité de signature de code AUTO-SIGNÉE STABLE (« Tabs Dev ») dans
# le trousseau de session, à exécuter UNE SEULE FOIS.
#
# Pourquoi : `make bundle` re-signe l'app à chaque fois. En signature ad-hoc,
# l'identité de code change à chaque build, donc macOS (TCC) oublie les
# permissions Accessibilité / Enregistrement de l'écran. Avec une identité
# stable, l'autorisation accordée une fois persiste entre les rebuilds.
#
# Le script peut demander le mot de passe du trousseau (interaction normale).
set -euo pipefail

NAME="Tabs Dev"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning 2>/dev/null | grep -q "$NAME"; then
  echo "[signing] identité « $NAME » déjà présente — rien à faire."
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/openssl.cnf" <<'CNF'
[req]
distinguished_name = dn
x509_extensions    = v3
prompt             = no
[dn]
CN = Tabs Dev
[v3]
basicConstraints     = critical,CA:false
keyUsage             = critical,digitalSignature
extendedKeyUsage     = critical,codeSigning
CNF

echo "[signing] génération du certificat auto-signé…"
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" \
  -config "$TMP/openssl.cnf" -extensions v3 >/dev/null 2>&1

openssl pkcs12 -export -name "$NAME" \
  -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -out "$TMP/id.p12" -passout pass:tabs >/dev/null 2>&1

echo "[signing] import dans le trousseau de session (peut demander ton mot de passe)…"
security import "$TMP/id.p12" -k "$KEYCHAIN" -P tabs -A -T /usr/bin/codesign

echo "[signing] identité « $NAME » créée."
echo "→ Relance « make bundle » : l'app sera signée de façon stable."
echo "→ Accorde les permissions UNE fois ; elles persisteront ensuite."
