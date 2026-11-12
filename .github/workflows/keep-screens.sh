#!/usr/bin/env bash
# keep-screens.sh <folder>: the Simulator's screens of a run, halved and in JPEG, as one commit at
# refs/axyon-screens/latest (not a branch; replaced each time), to fetch with git where an artifact cannot be
# downloaded. Needs TOKEN (contents: write), REPO and NOTE.
set -euo pipefail
cd "$1" 2>/dev/null || { echo "No screens"; exit 0; }
shopt -s nullglob
files=(*.png)
[ ${#files[@]} -gt 0 ] || { echo "No screens"; exit 0; }
python3 -m pip install --quiet --disable-pip-version-check pillow
python3 - "${files[@]}" <<'PY'
import sys
from PIL import Image
for name in sys.argv[1:]:
    image = Image.open(name).convert("RGB")
    image = image.resize((image.width // 2, image.height // 2), Image.Resampling.LANCZOS)
    image.save(name[:-4] + ".jpg", quality=84, optimize=True)
PY
rm -f -- "${files[@]}"
git init -q
git add .
git -c user.name="AXYON screens" -c user.email="actions@users.noreply.github.com" commit -qm "$NOTE"
git push -f "https://x-access-token:${TOKEN}@github.com/${REPO}" HEAD:refs/axyon-screens/latest
ls -l
