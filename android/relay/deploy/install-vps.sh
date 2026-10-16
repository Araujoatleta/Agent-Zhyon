#!/usr/bin/env sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
DOMAIN=${1:-}
[ -n "$DOMAIN" ] || { echo "usage: sudo sh deploy/install-vps.sh relay.example.com" >&2; exit 2; }
command -v apt-get >/dev/null 2>&1 || { echo "This installer supports Ubuntu/Debian; otherwise install Docker, Compose, curl, openssl and zip, then run ./vpsctl up" >&2; exit 1; }
if ! command -v docker >/dev/null 2>&1 || ! docker compose version >/dev/null 2>&1 || ! command -v zip >/dev/null 2>&1; then
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y docker.io docker-compose-v2 ca-certificates curl openssl zip
  systemctl enable --now docker
fi
docker compose version >/dev/null 2>&1 || { echo "Docker Compose v2 is required" >&2; exit 1; }

umask 077
cat > "$HERE/.env" <<EOF
COUCO_DOMAIN=$DOMAIN
COUCO_MODE=portable
COUCO_CONNECTS_PER_MINUTE=60
COUCO_WINDOWS_INSTALLER_URL=https://github.com/Araujoatleta/Agent-Zhyon/releases/download/windows-latest/Coucou-Windows-setup.exe
EOF
chmod 0600 "$HERE/.env"
"$HERE/vpsctl" up
echo "Next: $HERE/vpsctl client-package first-computer"

