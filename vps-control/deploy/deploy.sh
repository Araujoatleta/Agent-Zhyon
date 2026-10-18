#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  deploy.sh deploy [--source REPO] [--target DIR] --env-file FILE [--nginx-server-config FILE | --skip-nginx] [--control-health URL] [--relay-health URL] [--dry-run]
  deploy.sh rollback [--target DIR] [--release DIR] [--nginx-server-config FILE | --skip-nginx] [--control-health URL] [--relay-health URL] [--dry-run]
EOF
  exit 2
}

[[ $# -gt 0 ]] || usage
action=$1; shift
[[ "$action" == deploy || "$action" == rollback ]] || usage
source_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
target=/opt/agent-zhyon-control
env_file=""; nginx_server=""; skip_nginx=0; dry_run=0
control_health=http://127.0.0.1:8790/healthz; relay_health=""; release_override=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --source) source_dir=$2; shift 2;; --target) target=$2; shift 2;;
    --env-file) env_file=$2; shift 2;; --nginx-server-config) nginx_server=$2; shift 2;;
    --skip-nginx) skip_nginx=1; shift;; --control-health) control_health=$2; shift 2;;
    --relay-health) relay_health=$2; shift 2;; --release) release_override=$2; shift 2;;
    --dry-run) dry_run=1; shift;; *) usage;;
  esac
done

target=$(realpath -m "$target")
releases="$target/releases"
quote() { printf '%q ' "$@"; }
run() { printf '+ '; quote "$@"; printf '\n'; [[ $dry_run -eq 1 ]] || "$@"; }
health() {
  local url=$1 label=$2 attempt
  printf '+ health-check %s %s\n' "$label" "$url"
  [[ $dry_run -eq 1 ]] && return 0
  for attempt in {1..20}; do curl --fail --silent --show-error --max-time 5 "$url" >/dev/null && return 0; sleep 2; done
  echo "error: $label health check failed: $url" >&2; return 1
}
confine_release() {
  local candidate
  candidate=$(realpath -m "$1")
  case "$candidate" in "$releases"/*) printf '%s\n' "$candidate";; *) echo "error: release is outside target releases" >&2; return 2;; esac
}
compose() {
  local selected=$1
  run docker compose --project-name coucou-control --env-file "$selected/config/control.env" \
    --project-directory "$selected/vps-control" -f "$selected/vps-control/compose.yml" "${@:2}"
}
install_nginx() {
  local selected=$1
  [[ $skip_nginx -eq 1 ]] && return 0
  run install -o root -g root -m 0644 "$selected/config/nginx-control.conf" /etc/nginx/snippets/coucou-control.conf
  run nginx -t
  run systemctl reload nginx
}
activate() {
  local selected=$1
  compose "$selected" config --quiet
  compose "$selected" up -d --build --remove-orphans
  install_nginx "$selected"
}

if [[ "$action" == rollback && -n "$release_override" ]]; then release=$(confine_release "$release_override") || exit $?; fi
if [[ "$action" == deploy ]]; then
  [[ -n "$env_file" && -f "$env_file" ]] || { echo "error: deploy requires an existing --env-file" >&2; exit 2; }
  grep -Eq '^[[:space:]]*ADMIN_TOKEN=.{24,}[[:space:]]*$' "$env_file" || { echo "error: ADMIN_TOKEN must be at least 24 characters" >&2; exit 2; }
  [[ -f "$source_dir/vps-control/compose.yml" ]] || { echo "error: source is not an Agent-Zhyon checkout" >&2; exit 2; }
fi
if [[ $skip_nginx -eq 0 ]]; then
  [[ -n "$nginx_server" && -f "$nginx_server" ]] || { echo "error: provide --nginx-server-config or --skip-nginx" >&2; exit 2; }
  grep -Fq 'include /etc/nginx/snippets/coucou-control.conf;' "$nginx_server" || { echo "error: server config lacks required snippet include" >&2; exit 2; }
fi

# Serialize operators. Dry runs remain mutation-free.
if [[ $dry_run -eq 0 ]]; then
  mkdir -p "$target"
  exec 9>"$target/.deploy.lock"
  flock -n 9 || { echo "error: another deployment is running" >&2; exit 75; }
else
  printf '+ flock %s\n' "$target/.deploy.lock"
fi
[[ -z "$relay_health" ]] || health "$relay_health" existing-relay-before

previous=""
if [[ -L "$target/current" ]]; then previous=$(confine_release "$(readlink -f "$target/current")") || exit $?; fi

if [[ "$action" == deploy ]]; then
  stamp=$(date -u +%Y%m%dT%H%M%SZ); revision=$(git -C "$source_dir" rev-parse --short HEAD 2>/dev/null || printf local)
  if [[ $dry_run -eq 1 ]]; then release="$releases/$stamp-$revision-DRYRUN"; printf '+ mktemp -d %s\n' "$releases/$stamp-$revision-XXXXXX"
  else mkdir -p "$releases"; release=$(mktemp -d "$releases/$stamp-$revision-XXXXXX"); fi
  run mkdir -p "$release/vps-control" "$release/config" "$target/backups"
  run cp -a "$source_dir/vps-control/Dockerfile" "$source_dir/vps-control/compose.yml" \
    "$source_dir/vps-control/package.json" "$source_dir/vps-control/package-lock.json" \
    "$source_dir/vps-control/tsconfig.json" "$source_dir/vps-control/src" "$source_dir/vps-control/deploy" "$release/vps-control/"
  # Atomic, private, versioned environment install.
  run install -m 0600 "$env_file" "$release/config/control.env.tmp"
  run mv -f "$release/config/control.env.tmp" "$release/config/control.env"
  run cp "$source_dir/vps-control/deploy/nginx-control.conf" "$release/config/nginx-control.conf"
else
  if [[ -z "${release:-}" ]]; then
    [[ -f "$target/backups/previous-release" ]] || { echo "error: no previous release recorded" >&2; exit 2; }
    release=$(confine_release "$(<"$target/backups/previous-release")") || exit $?
  fi
  [[ $dry_run -eq 1 || -f "$release/vps-control/compose.yml" ]] || { echo "error: rollback release is invalid" >&2; exit 2; }
  [[ $dry_run -eq 1 || -f "$release/config/control.env" ]] || { echo "error: rollback release lacks versioned environment" >&2; exit 2; }
fi

if ! activate "$release" || ! health "$control_health" control-plane || { [[ -n "$relay_health" ]] && ! health "$relay_health" existing-relay-after; }; then
  if [[ "$action" == deploy && -n "$previous" ]]; then
    echo "warning: deployment failed; starting automatic rollback to $previous" >&2
    activate "$previous" || true
    health "$control_health" rollback-control-plane || true
  fi
  exit 1
fi

if [[ -n "$previous" && "$previous" != "$release" ]]; then
  if [[ $dry_run -eq 0 ]]; then printf '%s\n' "$previous" >"$target/backups/previous-release.tmp"; mv -f "$target/backups/previous-release.tmp" "$target/backups/previous-release"; fi
  printf '+ save previous release %s\n' "$previous"
fi
run ln -sfn "$release" "$target/current.new"
run mv -Tf "$target/current.new" "$target/current"
printf '{"action":"%s","release":"%s","dry_run":%s}\n' "$action" "$release" "$([[ $dry_run -eq 1 ]] && echo true || echo false)"
