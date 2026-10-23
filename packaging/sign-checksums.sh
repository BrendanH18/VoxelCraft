#!/bin/bash
# Signs a release's SHA256SUMS.txt for the in-game updater (docs/updates.md).
#
# The Ed25519 private key comes from $UPDATE_SIGNING_KEY (PEM, the GitHub
# Actions secret). The script refuses a key whose public half isn't listed
# in packaging/update-keys.txt, checks the signature, and writes it as hex
# to SHA256SUMS.txt.sig beside the list.
set -euo pipefail

sums="${1:?usage: sign-checksums.sh <SHA256SUMS.txt>}"
keys="$(cd "$(dirname "$0")" && pwd)/update-keys.txt"
if [ -z "${UPDATE_SIGNING_KEY:-}" ]; then
    echo "UPDATE_SIGNING_KEY is not set. Releases must be signed, or the in-game updater refuses them." >&2
    exit 1
fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
(umask 077 && printf '%s\n' "$UPDATE_SIGNING_KEY" > "$work/key.pem")

public="$(openssl pkey -in "$work/key.pem" -pubout -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n')"
if ! grep -qx "$public" "$keys"; then
    echo "The signing key's public half ($public) is not in packaging/update-keys.txt." >&2
    exit 1
fi
openssl pkeyutl -sign -inkey "$work/key.pem" -rawin -in "$sums" -out "$work/sig.bin"
openssl pkey -in "$work/key.pem" -pubout -out "$work/public.pem"
openssl pkeyutl -verify -pubin -inkey "$work/public.pem" -rawin -in "$sums" -sigfile "$work/sig.bin" >/dev/null
od -An -tx1 "$work/sig.bin" | tr -d ' \n' > "$sums.sig"
echo >> "$sums.sig"
echo "Signed $sums with key $public"
