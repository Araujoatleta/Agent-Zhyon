#!/usr/bin/env bash
# Hermes inside the iPhone app (README.md): what the IPA carries for it, built on macOS with Xcode, Rust and
# Python 3.14, before `xcodegen --spec project-hermes.yml`:
#   build/support/Python.xcframework   CPython 3.14 for iOS (BeeWare's Python-Apple-support, the build CPython's docs
#                                      point to)
#   build/app                          Hermes at the commit the computer installs, its iOS stand-ins and the host
#   build/app_packages                 its packages: pure Python, iOS wheels, and pydantic-core and jiter built here
#                                      for arm64 iOS (they publish no iOS wheel)
# Every .py is compiled to bytecode that never goes stale (the bundle is signed: nothing is written in it).
set -euo pipefail
cd "$(dirname "$0")"
HERE="$(pwd)"
OUT="$HERE/build"
PYTHON="${PYTHON:-python3.14}"
# The Hermes Agent commit the computer installs (windows/src-tauri/src/hermes_setup.rs).
HERMES_COMMIT="$(sed -n 's/^pub const COMMIT: &str = "\([0-9a-f]\{40\}\)";$/\1/p' "$HERE/../../windows/src-tauri/src/hermes_setup.rs")"
IOS_TARGET=13.0
PYDANTIC_CORE=2.46.4
JITER=0.16.0

test -n "$HERMES_COMMIT" || { echo "Hermes' commit not found in hermes_setup.rs"; exit 1; }
"$PYTHON" -c 'import sys; assert sys.version_info[:2] == (3, 14), sys.version' || { echo "$PYTHON is not Python 3.14"; exit 1; }
mkdir -p "$OUT"
# A Python of this build's own for pip (a system or Homebrew Python refuses installs, PEP 668).
if [ ! -x "$OUT/venv/bin/python" ]; then "$PYTHON" -m venv "$OUT/venv"; fi
PYTHON="$OUT/venv/bin/python"

step() { echo; echo "── $* ──"; }

# ── CPython for iOS ──────────────────────────────────────────────────────────
step "Python 3.14 for iOS"
XCF="$OUT/support/Python.xcframework"
if [ ! -d "$XCF" ]; then
    rm -rf "$OUT/support" && mkdir -p "$OUT/support"
    AUTH=()
    if [ -n "${GITHUB_TOKEN:-}" ]; then AUTH=(-H "Authorization: Bearer $GITHUB_TOKEN"); fi
    URL="$(curl -fsSL "${AUTH[@]}" "https://api.github.com/repos/beeware/Python-Apple-support/releases?per_page=50" | "$PYTHON" -c '
import json, sys
for release in json.load(sys.stdin):
    if release.get("draft") or not release["tag_name"].startswith("3.14-"):
        continue
    for asset in release["assets"]:
        name = asset["name"]
        if name.startswith("Python-3.14-iOS-support.") and name.endswith(".tar.gz"):
            print(asset["browser_download_url"])
            sys.exit()
sys.exit("no Python 3.14 iOS support package among the releases")
')"
    echo "$URL"
    curl -fsSL "$URL" -o "$OUT/support.tar.gz"
    tar -xzf "$OUT/support.tar.gz" -C "$OUT/support"
    rm "$OUT/support.tar.gz"
fi
SLICE="$XCF/ios-arm64"
test -d "$SLICE/Python.framework" || { echo "no device slice in the XCframework"; ls -R "$XCF" | head -50; exit 1; }
SYSCONFIG="$(find "$XCF" -name '_sysconfigdata__ios_arm64-iphoneos.py' | head -1)"
EXT_SUFFIX="$("$PYTHON" -c "import runpy, sys; print(runpy.run_path(sys.argv[1])['build_time_vars']['EXT_SUFFIX'])" "$SYSCONFIG")"
echo "XCframework at $XCF, extension modules end in $EXT_SUFFIX"

# ── Hermes and the host ──────────────────────────────────────────────────────
step "Hermes Agent at $HERMES_COMMIT"
SRC="$OUT/hermes-src"
if [ ! -d "$SRC/.git" ]; then
    git init -q "$SRC"
    git -C "$SRC" remote add origin https://github.com/NousResearch/hermes-agent
fi
git -C "$SRC" fetch -q --depth 1 origin "$HERMES_COMMIT"
git -C "$SRC" checkout -q --force FETCH_HEAD
APP="$OUT/app"
rm -rf "$APP" && mkdir -p "$APP"
# Hermes' own Python and what it reads at run time; its tests, website, apps and tooling stay out.
for dir in acp_adapter agent cron gateway hermes_cli hermes_platform locales plugin-catalog plugins pm providers skills tools; do
    rsync -a --exclude '__pycache__' --exclude 'tests' --exclude '*.pyc' "$SRC/$dir" "$APP/"
done
cp "$SRC"/*.py "$SRC/pyproject.toml" "$SRC/SOUL.md" "$SRC/cli-config.yaml.example" "$SRC/LICENSE" "$APP/"
rsync -a --exclude '__pycache__' "$HERE/app/axyon_hermes" "$APP/"
rsync -a --exclude '__pycache__' "$HERE/shims/" "$APP/"

# ── Packages ─────────────────────────────────────────────────────────────────
step "Packages"
PKG="$OUT/app_packages"
rm -rf "$PKG" && mkdir -p "$PKG"
"$PYTHON" -m pip install -q --disable-pip-version-check --no-compile --target "$PKG" --no-deps \
    --only-binary=:all: --platform any --implementation py --abi none --python-version 3.14 \
    -r "$HERE/requirements-pure.txt"
"$PYTHON" -m pip install -q --disable-pip-version-check --no-compile --target "$PKG" --no-deps \
    --only-binary=:all: --platform ios_13_0_arm64_iphoneos --implementation cp --abi cp314 --python-version 3.14 \
    -r "$HERE/requirements-ios-wheels.txt"

step "pydantic-core $PYDANTIC_CORE and jiter $JITER for arm64 iOS"
rustup target add aarch64-apple-ios >/dev/null
RUST="$OUT/rust"
mkdir -p "$RUST"
cat > "$RUST/pyo3.cfg" <<CFG
implementation=CPython
version=3.14
shared=true
abi3=false
lib_name=python3.14
lib_dir=$SLICE
pointer_width=64
build_flags=
suppress_build_script_link_lines=true
CFG
export PYO3_CONFIG_FILE="$RUST/pyo3.cfg"
export IPHONEOS_DEPLOYMENT_TARGET="$IOS_TARGET"
# One target folder for both, which CI keeps between runs.
export CARGO_TARGET_DIR="$RUST/target"
BUILT="$CARGO_TARGET_DIR/aarch64-apple-ios/release"
# Linked against Python.framework, which the app embeds, rather than left to look its symbols up at load.
export CARGO_TARGET_AARCH64_APPLE_IOS_RUSTFLAGS="-C link-arg=-F$SLICE -C link-arg=-framework -C link-arg=Python"

sdist() {  # name version → the unpacked sdist's folder
    local name=$1 version=$2
    local url
    url="$(curl -fsSL "https://pypi.org/pypi/$name/$version/json" | "$PYTHON" -c '
import json, sys
print(next(u["url"] for u in json.load(sys.stdin)["urls"] if u["packagetype"] == "sdist"))')"
    curl -fsSL "$url" -o "$RUST/$name.tar.gz"
    tar -xzf "$RUST/$name.tar.gz" -C "$RUST"
    echo "$RUST/$(tar -tzf "$RUST/$name.tar.gz" | head -1 | cut -d/ -f1)"
}

dist_info() {  # package version → a minimal .dist-info, so importlib.metadata finds it
    local name=$1 version=$2
    local dir="$PKG/${name}-${version}.dist-info"
    mkdir -p "$dir"
    printf 'Metadata-Version: 2.1\nName: %s\nVersion: %s\n' "$name" "$version" > "$dir/METADATA"
    : > "$dir/RECORD"
    echo "built-for-ios" > "$dir/INSTALLER"
}

PC="$(sdist pydantic-core "$PYDANTIC_CORE")"
(cd "$PC" && cargo build --release --locked --target aarch64-apple-ios --lib)
mkdir -p "$PKG/pydantic_core"
cp "$PC"/python/pydantic_core/*.py "$PC"/python/pydantic_core/*.pyi "$PC"/python/pydantic_core/py.typed "$PKG/pydantic_core/"
cp "$BUILT/lib_pydantic_core.dylib" "$PKG/pydantic_core/_pydantic_core$EXT_SUFFIX"
dist_info pydantic_core "$PYDANTIC_CORE"

JT="$(sdist jiter "$JITER")"
(cd "$JT" && cargo build --release --locked --target aarch64-apple-ios -p jiter-python)
mkdir -p "$PKG/jiter"
cat > "$PKG/jiter/__init__.py" <<'PY'
from .jiter import *

__doc__ = jiter.__doc__
if hasattr(jiter, "__all__"):
    __all__ = jiter.__all__
PY
cp "$JT/crates/jiter-python/jiter.pyi" "$PKG/jiter/__init__.pyi"
touch "$PKG/jiter/py.typed"
cp "$BUILT/libjiter_python.dylib" "$PKG/jiter/jiter$EXT_SUFFIX"
dist_info jiter "$JITER"

# ── Checks ───────────────────────────────────────────────────────────────────
step "Every compiled module is arm64 iOS"
bad=0
while IFS= read -r so; do
    platform="$(xcrun vtool -show-build "$so" 2>/dev/null | awk '/platform/ {print $2; exit}')"
    arch="$(lipo -archs "$so" 2>/dev/null || true)"
    if [ "$platform" != "IOS" ] || [ "$arch" != "arm64" ]; then
        echo "  ✗ $so: platform '$platform', arch '$arch'"
        bad=1
    fi
done < <(find "$PKG" "$APP" -name '*.so')
test "$bad" = 0 || exit 1
find "$PKG" -name '*.so' | sed "s|$OUT/||" | sort

step "Bytecode"
"$PYTHON" -m compileall -q -j 0 --invalidation-mode unchecked-hash "$APP" "$PKG" >/dev/null || true
du -sh "$XCF" "$APP" "$PKG"
echo "Hermes for iOS ready in $OUT"
