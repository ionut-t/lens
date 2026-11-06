#!/usr/bin/env bash
# Re-record assets/demo.gif.
#
#   ./demo/record.sh
#
# Requires: vhs, lens, node/npm on PATH.
#
# The fixture is copied to a neutral working directory (default
# /tmp/lens-demo/shop) before every run so that:
#   - absolute paths in Vitest stack traces don't reveal where the repo lives,
#   - the fix typed during the recording never touches demo/fixture.
# The recorded shell runs with an isolated HOME (demo/.tmp/home) and a minimal
# environment, so no personal config, prompt or paths end up in the GIF.
set -euo pipefail

DEMO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$DEMO_DIR")"
WORK_DIR="${LENS_DEMO_WORKDIR:-/tmp/lens-demo/shop}"
DEMO_HOME="$DEMO_DIR/.tmp/home"

for bin in vhs lens node npm vim; do
  command -v "$bin" >/dev/null || { echo "record.sh: '$bin' not found on PATH" >&2; exit 1; }
done

echo "==> Installing fixture dependencies"
(cd "$DEMO_DIR/fixture" && npm install --no-audit --no-fund --silent)

echo "==> Preparing $WORK_DIR"
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"
cp -R "$DEMO_DIR/fixture/." "$WORK_DIR/"

rm -rf "$DEMO_HOME"
mkdir -p "$DEMO_HOME"

# Minimal PATH for the recorded shell: lens, node/npm, system tools (vim).
LENS_DEMO_PATH="$(dirname "$(command -v lens)"):$(dirname "$(command -v node)"):/usr/bin:/bin"

export LENS_DEMO_HOME="$DEMO_HOME"
export LENS_DEMO_WORKDIR="$WORK_DIR"
export LENS_DEMO_PATH

echo "==> Recording"
mkdir -p "$REPO_DIR/assets"
cd "$REPO_DIR"
vhs demo/demo.tape

echo "==> Checking recording for personal paths"
leaks=$(grep -nF -e "$HOME" -e "$REPO_DIR" -e "$(id -un)" demo/.tmp/demo.ascii || true)
if [[ -n "$leaks" ]]; then
  echo "record.sh: recording leaks personal info:" >&2
  echo "$leaks" >&2
  exit 1
fi

echo "==> Wrote assets/demo.gif"
