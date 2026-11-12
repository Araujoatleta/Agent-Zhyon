#!/usr/bin/env bash
# The local brain's catalogue (Swift/AXYONLocalBrain.swift): every model's link answers with a GGUF file, of the size
# the screen says, about. Run by CI once the .ipa is built: it tells what it found and never holds the build back.
set -uo pipefail
cd "$(dirname "$0")/.."
bad=0
while IFS= read -r url; do
    # A GGUF file's first four bytes are its name.
    magic="$(curl -fsSL --max-time 45 -r 0-3 "$url" 2>/dev/null | head -c 4 | tr -d '\0')"
    # The size, from the last answer of the redirects (Hugging Face sends the file from another host).
    size="$(curl -fsSIL --max-time 45 "$url" 2>/dev/null | tr -d '\r' | awk 'tolower($1) == "content-length:" { n = $2 } END { printf "%d", n }')"
    if [ "$magic" = GGUF ]; then
        awk -v n="$size" -v u="$url" 'BEGIN { printf "ok   %6.2f GB  %s\n", n / 1e9, u }'
    else
        echo "::warning::the local brain's model link does not answer with a GGUF file: $url"
        bad=1
    fi
done < <(grep -o 'https://huggingface.co/[^"]*\.gguf' Swift/AXYONLocalBrain.swift)
exit "$bad"
