#!/usr/bin/env sh
set -eu

if [ -z "${ACCESS_KEY:-}" ]; then
  SOURCE=${ACCESS_KEY_FILE:-/run/secrets/coucou_relay_access}
  TARGET=/tmp/coucou-relay-access-key

  cp "$SOURCE" "$TARGET"
  chown node:node "$TARGET"
  chmod 0400 "$TARGET"
  export ACCESS_KEY_FILE="$TARGET"
fi

exec su-exec node:node "$@"
