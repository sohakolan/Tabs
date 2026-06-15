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

# macOS `security import` ne lit que les PKCS12 « legacy » : MAC SHA-1 +
# chiffrement 3DES/RC2. OpenSSL 3 génère par défaut un MAC SHA-256 + AES, refusé
# par macOS (« MAC verification failed »). On force donc les algos legacy quand
# l'option existe (OpenSSL 3) ; LibreSSL les produit déjà par défaut.
P12_OPTS=()
if openssl pkcs12 -help 2>&1 | grep -q -- '-legacy'; then
  P12_OPTS+=(-legacy -macalg sha1)
fi
openssl pkcs12 -export -name "$NAME" \
  -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -out "$TMP/id.p12" -passout pass:tabs "${P12_OPTS[@]}" >/dev/null 2>&1

echo "[signing] import dans le trousseau de session (peut demander ton mot de passe)…"
security import "$TMP/id.p12" -k "$KEYCHAIN" -P tabs -A -T /usr/bin/codesign

echo "[signing] identité « $NAME » créée."
echo "→ Relance « make bundle » : l'app sera signée de façon stable."
echo "→ Accorde les permissions UNE fois ; elles persisteront ensuite."
