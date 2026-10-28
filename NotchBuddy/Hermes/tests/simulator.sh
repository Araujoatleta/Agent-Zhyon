#!/usr/bin/env bash
# Hermes inside iOS for real, in the Simulator (CI, job hermes-simulator): the app built with HERMES for the Simulator
# is installed and launched, its host must say it is ready, then answers a question with a tool the app itself
# answers over the bridge (iphone_status), the model being tests/fake_model.py on this Mac. The app's own logs and
# Hermes' traceback, if any, are printed when something fails.
#   bash simulator.sh path/to/Coucou.app
set -euo pipefail
APP="$1"
HERE="$(cd "$(dirname "$0")" && pwd)"
PYTHON="${PYTHON:-python3}"
BUNDLE=fr.louisraille.Coucou
PORT=47911
MODEL_PORT=47912
TOKEN="ci-$(openssl rand -hex 16)"
URL="http://127.0.0.1:$PORT"

DEVICE="$(xcrun simctl list devices available -j | "$PYTHON" -c '
import json, sys
runtimes = json.load(sys.stdin)["devices"]
for runtime in sorted(runtimes, reverse=True):
    if "iOS" not in runtime:
        continue
    for d in runtimes[runtime]:
        if d["name"].startswith("iPhone"):
            print(d["udid"])
            sys.exit()
sys.exit("no iPhone simulator")
')"
echo "Simulator $DEVICE"

"$PYTHON" "$HERE/fake_model.py" "$MODEL_PORT" 2>"$RUNNER_TEMP/fake-model.log" &
MODEL_PID=$!

show_logs() {
    echo "── The app's log ──"
    xcrun simctl spawn "$DEVICE" log show --last 5m --style compact \
        --predicate 'process == "Coucou"' 2>/dev/null | tail -150 || true
    DATA="$(xcrun simctl get_app_container "$DEVICE" "$BUNDLE" data 2>/dev/null || true)"
    if [ -n "$DATA" ]; then
        echo "── Hermes' folder ──"
        ls -la "$DATA/Library/Application Support/Hermes" 2>/dev/null || true
        if [ -f "$DATA/Library/Application Support/Hermes/host-error.txt" ]; then
            echo "── host-error.txt ──"
            cat "$DATA/Library/Application Support/Hermes/host-error.txt"
        fi
    fi
    echo "── The model's log ──"
    cat "$RUNNER_TEMP/fake-model.log" || true
}

fail() {
    echo "::error::$*"
    show_logs
    kill "$MODEL_PID" 2>/dev/null || true
    exit 1
}

say() { echo "[$(date +%H:%M:%S)] $*"; }

say "Booting the Simulator"
xcrun simctl boot "$DEVICE" 2>/dev/null || true
booted=""
for _ in $(seq 1 120); do
    if xcrun simctl list devices | grep "$DEVICE" | grep -q "(Booted)"; then booted=1; break; fi
    sleep 3
done
[ -n "$booted" ] || fail "the Simulator did not boot in 6 minutes"
# Its first boot finishes setting itself up after it says Booted.
sleep 20
say "Installing and launching AXYON"
xcrun simctl install "$DEVICE" "$APP"
SIMCTL_CHILD_AXYON_TEST_PORT="$PORT" SIMCTL_CHILD_AXYON_TEST_TOKEN="$TOKEN" \
    xcrun simctl launch "$DEVICE" "$BUNDLE" || fail "the app did not launch"

call() {  # method path [json body]
    if [ $# -ge 3 ]; then
        curl -sS --max-time 60 -X "$1" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
            --data "$3" "$URL$2"
    else
        curl -sS --max-time 60 -X "$1" -H "Authorization: Bearer $TOKEN" "$URL$2"
    fi
}

say "Waiting for Hermes to be ready inside iOS…"
started=$(date +%s)
ready=""
for i in $(seq 1 150); do
    health="$(curl -sS --max-time 5 -H "Authorization: Bearer $TOKEN" "$URL/health" 2>&1 || true)"
    if [ $((i % 10)) = 0 ]; then say "still waiting: ${health:0:200}"; fi
    state="$(printf '%s' "$health" | "$PYTHON" -c '
import json, sys
try:
    h = json.load(sys.stdin)
except ValueError:
    sys.exit()
print("ready" if h.get("ready") else ("problem: " + h["problem"]) if h.get("problem") else "starting")
' 2>/dev/null || true)"
    case "$state" in
        ready) ready=1; break ;;
        problem:*) fail "Hermes did not start inside iOS: ${state#problem: }" ;;
    esac
    sleep 2
done
[ -n "$ready" ] || fail "Hermes was not ready inside iOS after 5 minutes (last answer: ${health:-none})"
say "Hermes ready inside iOS in $(( $(date +%s) - started ))s: $health"

echo "The brain: the local stand-in model"
call POST /v1/settings "{\"provider\":\"custom\",\"model\":\"fake-model\",\"baseUrl\":\"http://127.0.0.1:$MODEL_PORT/v1\",\"keys\":{\"OPENAI_API_KEY\":\"not-a-real-key\"}}"
echo

echo "A question that takes a tool of the iPhone"
run="$(call POST /v1/runs '{"session":"ci-simulator","message":"Como está o meu iPhone?"}' | "$PYTHON" -c 'import json,sys; print(json.load(sys.stdin)["run_id"])')" \
    || fail "the run did not start"
events="$(curl -sS --max-time 180 -H "Authorization: Bearer $TOKEN" "$URL/v1/runs/$run/events" || true)"
echo "$events" | grep '^data: ' | cut -c1-400
printf '%s' "$events" | "$PYTHON" -c '
import json, sys
events = [json.loads(line[6:]) for line in sys.stdin.read().splitlines() if line.startswith("data: ")]
kinds = [e["type"] for e in events]
tools = [e.get("name") for e in events if e["type"] == "tool"]
done = [e for e in events if e["type"] == "done"]
errors = [e for e in events if e["type"] == "error"]
assert not errors, errors
assert "iphone_status" in tools, kinds
assert done and "Feito:" in done[0]["text"], done
said = done[0]["text"]
assert "model" in said or "system" in said, said
print("The app answered Hermes over the bridge:", said[:300])
' || fail "the answer was not the one expected"

echo "The portfolio and the brain's settings answer"
call GET /v1/settings | head -c 600
echo
call GET /v1/portfolio | head -c 400
echo

kill "$MODEL_PID" 2>/dev/null || true
xcrun simctl terminate "$DEVICE" "$BUNDLE" 2>/dev/null || true
echo "Hermes runs inside iOS: started, answered, used the iPhone's own tool."
