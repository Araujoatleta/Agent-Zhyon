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
OUT_LOG="$RUNNER_TEMP/app-stdout.log"
ERR_LOG="$RUNNER_TEMP/app-stderr.log"

# What the app keeps in its own folder: Hermes' and the superpowers'.
data_dir() { xcrun simctl get_app_container "$DEVICE" "$BUNDLE" data 2>/dev/null || true; }

show_logs() {
    echo "── The app's output (AXYON's steps, Python's errors) ──"
    tail -80 "$ERR_LOG" 2>/dev/null || true
    tail -20 "$OUT_LOG" 2>/dev/null || true
    echo "── The app's log, without iOS' own ──"
    xcrun simctl spawn "$DEVICE" log show --last 20m --info --debug --style compact \
        --predicate 'process == "Coucou" AND NOT (subsystem BEGINSWITH "com.apple")' 2>/dev/null | tail -150 || true
    DATA="$(data_dir)"
    if [ -n "$DATA" ]; then
        echo "── Hermes' folder ──"
        ls -la "$DATA/Library/Application Support/Hermes" 2>/dev/null || true
        for f in python-ended.txt host-error.txt; do
            if [ -f "$DATA/Library/Application Support/Hermes/$f" ]; then
                echo "── $f ──"
                cat "$DATA/Library/Application Support/Hermes/$f"
            fi
        done
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

say "What the app carries for Hermes"
test -f "$APP/python/lib/python3.14/os.py" || fail "no standard library in the app"
test -f "$APP/app/axyon_hermes/host.py" || fail "no host in the app"
test -f "$APP/app/axyon_hermes/house_tools.json" || fail "no superpowers' tools in the app"
test -d "$APP/Frameworks/Python.framework" || fail "no Python.framework in the app"
echo "$(ls "$APP/Frameworks" | wc -l | tr -d ' ') frameworks; $(find "$APP/python/lib/python3.14/lib-dynload" -name '*.fwork' | wc -l | tr -d ' ') standard modules moved to them"
nm -g "$APP/Coucou" 2>/dev/null | grep -E "axyon_(python_run|powers_call)" || echo "(the app's own symbols are stripped)"

say "Booting the Simulator"
xcrun simctl boot "$DEVICE" 2>/dev/null || true
booted=""
for _ in $(seq 1 120); do
    if xcrun simctl list devices | grep "$DEVICE" | grep "(Booted)" >/dev/null; then booted=1; break; fi
    sleep 3
done
[ -n "$booted" ] || fail "the Simulator did not boot in 6 minutes"
# Its first boot finishes setting itself up after it says Booted.
sleep 20
say "Installing and launching AXYON"
xcrun simctl install "$DEVICE" "$APP"
SIMCTL_CHILD_AXYON_TEST_PORT="$PORT" SIMCTL_CHILD_AXYON_TEST_TOKEN="$TOKEN" \
    xcrun simctl launch --stdout="$OUT_LOG" --stderr="$ERR_LOG" "$DEVICE" "$BUNDLE" || fail "the app did not launch"

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
    if [ $((i % 10)) = 0 ]; then
        say "still waiting: ${health:0:200}"
        grep "AXYON:" "$ERR_LOG" 2>/dev/null | tail -3 || true
        # Python stopped: no use waiting for it.
        ended="$(data_dir)/Library/Application Support/Hermes/python-ended.txt"
        [ ! -f "$ended" ] || fail "Python stopped inside iOS: $(cat "$ended")"
    fi
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

echo "A superpower: the computer's engine inside the app, writing its own record"
run="$(call POST /v1/runs '{"session":"ci-simulator-powers","message":"Registra 15 minutos de inglês"}' | "$PYTHON" -c 'import json,sys; print(json.load(sys.stdin)["run_id"])')" \
    || fail "the superpowers' run did not start"
events="$(curl -sS --max-time 180 -H "Authorization: Bearer $TOKEN" "$URL/v1/runs/$run/events" || true)"
echo "$events" | grep '^data: ' | cut -c1-400
printf '%s' "$events" | "$PYTHON" -c '
import json, sys
events = [json.loads(line[6:]) for line in sys.stdin.read().splitlines() if line.startswith("data: ")]
tools = [e.get("name") for e in events if e["type"] == "tool"]
done = [e for e in events if e["type"] == "done"]
assert "life_english" in tools, [e["type"] for e in events]
assert done and "15 min of English logged" in done[0]["text"], done
print("The engine answered inside iOS:", done[0]["text"][:300])
' || fail "the superpowers did not answer as expected"
POWERS="$(data_dir)/Library/Application Support/AXYON/powers/powers.json"
test -s "$POWERS" || fail "the superpowers kept nothing in the app's folder"
grep -q '"streakDays":1' "$POWERS" || { head -c 600 "$POWERS"; fail "the English streak was not kept"; }
echo "Kept in the app's folder: $(wc -c < "$POWERS" | tr -d ' ') bytes"

echo "The portfolio and the brain's settings answer"
call GET /v1/settings | head -c 600
echo
call GET /v1/portfolio | head -c 400
echo

kill "$MODEL_PID" 2>/dev/null || true
xcrun simctl terminate "$DEVICE" "$BUNDLE" 2>/dev/null || true
echo "Hermes runs inside iOS: started, answered, used the iPhone's own tool."
