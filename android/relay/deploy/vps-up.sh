#!/usr/bin/env sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$HERE"

command -v docker >/dev/null 2>&1 || { echo "Docker is required" >&2; exit 1; }
command -v openssl >/dev/null 2>&1 || { echo "OpenSSL is required" >&2; exit 1; }
docker compose version >/dev/null 2>&1 || { echo "Docker Compose v2 is required" >&2; exit 1; }

umask 077
mkdir -p state/clients
if [ -s secrets/access-key ] && [ ! -s state/clients/default.key ]; then
  mv secrets/access-key state/clients/default.key
fi
if [ ! -s state/clients/default.key ] && ! find state/clients -type f -name '*.key' -print -quit | grep -q .; then
  openssl rand -base64 32 | tr '+/' '-_' | tr -d '=\n' > state/clients/default.key
fi

for key in state/clients/*.key; do
  [ -f "$key" ] || continue
  KEY_LENGTH=$(wc -c < "$key" | tr -d ' ')
  [ "$KEY_LENGTH" = "43" ] || { echo "$key must contain one 43-character base64url key" >&2; exit 1; }
done

# Docker Compose implements file-backed secrets as bind mounts, preserving the
# host file owner and mode. The official Node image runs as uid/gid 1000.
chmod 0700 state state/clients
chown 1000:1000 state/clients state/clients/*.key
chmod 0400 state/clients/*.key

docker compose -f compose.vps.yml up -d --build

i=0
until curl -fsS http://127.0.0.1:${COUCO_RELAY_PORT:-8787}/ >/dev/null; do
  i=$((i + 1))
  [ "$i" -lt 20 ] || { echo "Relay did not become healthy" >&2; exit 1; }
  sleep 1
done

echo "Coucou relay is healthy on 127.0.0.1:${COUCO_RELAY_PORT:-8787}."
echo "Keep state/clients private; configure TLS before using it from the internet."
