#!/bin/sh
# One-time setup: creates a local, self-signed code-signing identity called
# "VoiceDrop Local Signing" in your login keychain.
#
# Why: macOS ties the Accessibility permission to the app's code signature.
# An ad-hoc signature changes on every rebuild, so macOS silently forgets the
# permission and the hotkey stops working (issue #71). A fixed certificate
# keeps the signature's identity stable, so the permission survives rebuilds.
#
# The certificate is local only: it isn't trusted by anyone else, can't be
# used to distribute the app, and isn't tied to any Apple account. It's valid
# for 10 years. The first build that uses it may show a "codesign wants to
# use your keychain" dialog: click "Always Allow".
set -eu

NAME="VoiceDrop Local Signing"

if security find-certificate -c "$NAME" >/dev/null 2>&1; then
    echo "\"$NAME\" already exists in your keychain. Nothing to do."
    exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

cat >"$WORK/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -keyout "$WORK/key.pem" -out "$WORK/cert.pem" -config "$WORK/cert.cnf" 2>/dev/null

# -legacy: the macOS keychain can't read PKCS#12 files using OpenSSL 3+'s
# default encryption.
P12_PASS="voicedrop"
openssl pkcs12 -export -legacy -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
    -out "$WORK/identity.p12" -passout "pass:$P12_PASS"

# -T lets codesign use the private key without prompting every build.
security import "$WORK/identity.p12" -P "$P12_PASS" -T /usr/bin/codesign >/dev/null

echo "Created \"$NAME\" in your login keychain."
echo "Next: ./scripts/install.sh"
