#!/usr/bin/env sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$HERE"

command -v docker >/dev/null 2>&1 || { echo "Docker is required" >&2; exit 1; }
command -v openssl >/dev/null 2>&1 || { echo "OpenSSL is required" >&2; exit 1; }
docker compose version >/dev/null 2>&1 || { echo "Docker Compose v2 is required" >&2; exit 1; }

umask 077
mkdir -p secrets
if [ ! -s secrets/access-key ]; then
  openssl rand -base64 32 | tr '+/' '-_' | tr -d '=\n' > secrets/access-key
fi

KEY_LENGTH=$(wc -c < secrets/access-key | tr -d ' ')
[ "$KEY_LENGTH" = "43" ] || { echo "secrets/access-key must contain one 43-character base64url key" >&2; exit 1; }

# Docker Compose implements file-backed secrets as bind mounts, preserving the
# host file owner and mode. The official Node image runs as uid/gid 1000.
chmod 0700 secrets
chown 1000:1000 secrets/access-key
chmod 0400 secrets/access-key

docker compose -f compose.vps.yml up -d --build

i=0
until curl -fsS http://127.0.0.1:${COUCO_RELAY_PORT:-8787}/ >/dev/null; do
  i=$((i + 1))
  [ "$i" -lt 20 ] || { echo "Relay did not become healthy" >&2; exit 1; }
  sleep 1
done

echo "Coucou relay is healthy on 127.0.0.1:${COUCO_RELAY_PORT:-8787}."
echo "Keep secrets/access-key private; configure TLS before using it from the internet."
