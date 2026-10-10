#!/usr/bin/env bash
# Builds Leela Chess Zero (lc0) for macOS and installs it as a Tauri sidecar
# in src-tauri/binaries/lc0-<target-triple>.
#
# Usage:
#   engine/build-lc0.sh                 # build for this Mac's architecture
#   engine/build-lc0.sh --arch arm64    # cross-compile for Apple Silicon
#   engine/build-lc0.sh --arch x86_64   # build for Intel
#   engine/build-lc0.sh --universal     # build both and join them with lipo
#   engine/build-lc0.sh --prebuilt      # use the official universal v0.32.0 binary instead
#   engine/build-lc0.sh --clean ...     # wipe build directories first
#
# Meson and Ninja come from engine/.venv (created on first run) so no
# system-wide installation is needed.

set -euo pipefail

ENGINE_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(dirname -- "$ENGINE_DIR")"
LC0_DIR="$ENGINE_DIR/lc0"
BIN_DIR="$ROOT_DIR/src-tauri/binaries"
VENV="$ENGINE_DIR/.venv"
DEPLOYMENT_TARGET="13.0"
PREBUILT_URL="https://github.com/LeelaChessZero/lc0/releases/download/v0.32.0/lc0-v0.32.0-macos_12.6.1"

host_arch="$(uname -m)"
arches=()
clean=0
prebuilt=0

while [ $# -gt 0 ]; do
  case "$1" in
    --arch) arches+=("$2"); shift 2 ;;
    --universal) arches=(x86_64 arm64); shift ;;
    --prebuilt) prebuilt=1; shift ;;
    --clean) clean=1; shift ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ ${#arches[@]} -eq 0 ] && arches=("$host_arch")

triple_for() {
  case "$1" in
    x86_64) echo "x86_64-apple-darwin" ;;
    arm64|aarch64) echo "aarch64-apple-darwin" ;;
    universal) echo "universal-apple-darwin" ;;
    *) echo "unsupported arch: $1" >&2; exit 2 ;;
  esac
}

install_sidecar() { # <source> <arch-name>
  local dest="$BIN_DIR/lc0-$(triple_for "$2")"
  mkdir -p "$BIN_DIR"
  cp "$1" "$dest"
  chmod 755 "$dest"
  echo "installed $dest"
  lipo -info "$dest"
}

if [ "$prebuilt" -eq 1 ]; then
  tmp="$(mktemp -d)"
  echo "downloading $PREBUILT_URL"
  curl -fL --progress-bar -o "$tmp/lc0" "$PREBUILT_URL"
  chmod 755 "$tmp/lc0"
  lipo -info "$tmp/lc0"
  install_sidecar "$tmp/lc0" universal
  for a in x86_64 arm64; do
    lipo "$tmp/lc0" -thin "$a" -output "$tmp/lc0-$a"
    install_sidecar "$tmp/lc0-$a" "$a"
  done
  rm -rf "$tmp"
  exit 0
fi

if [ ! -f "$LC0_DIR/meson.build" ]; then
  echo "lc0 source missing; run: git submodule update --init --recursive" >&2
  exit 1
fi

if [ ! -x "$VENV/bin/meson" ] || [ ! -x "$VENV/bin/ninja" ] || ! "$VENV/bin/python" -c "import truststore" 2>/dev/null; then
  echo "creating build tools venv in $VENV"
  python3 -m venv "$VENV"
  # truststore lets meson verify HTTPS downloads with the macOS keychain.
  "$VENV/bin/pip" install --quiet --upgrade pip meson ninja truststore
fi
export PATH="$VENV/bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"

meson_opts=(
  --buildtype=release
  -Dgtest=false
  -Dnative_arch=false
  -Dispc=false
  -Donnx=false
  -Dmetal=enabled
  -Dblas=true
  -Daccelerate=true
)

# Apple clang searches /usr/local/include ahead of any -isystem directory.
# On Intel Macs with Homebrew that lets a system abseil shadow the copy lc0
# bundles, and linking fails. -idirafter moves /usr/local/include to the end
# of the search order so bundled headers always win.
built=()
for arch in "${arches[@]}"; do
  [ "$arch" = "aarch64" ] && arch=arm64
  build_dir="$LC0_DIR/build/grandie-$arch"
  [ "$clean" -eq 1 ] && rm -rf "$build_dir"
  extra=()
  if [ "$arch" != "$host_arch" ]; then
    case "$arch" in
      arm64) extra+=(--cross-file "$LC0_DIR/cross-files/aarch64-darwin") ;;
      x86_64) extra+=(--cross-file "$LC0_DIR/cross-files/x86_64-darwin") ;;
    esac
  fi
  # Command-line compiler args replace the cross file's, so -arch is repeated here.
  compile_args="['-arch', '$arch', '-idirafter', '/usr/local/include']"
  if [ ! -f "$build_dir/build.ninja" ]; then
    (cd "$LC0_DIR" && meson setup "$build_dir" "${meson_opts[@]}" "${extra[@]+"${extra[@]}"}" \
      "-Dcpp_args=$compile_args" "-Dobjcpp_args=$compile_args")
  fi
  meson compile -C "$build_dir"
  out="$build_dir/lc0"
  file "$out"
  minos="$(vtool -show-build "$out" 2>/dev/null | awk '/minos/ {print $2; exit}')"
  echo "minimum macOS for $arch build: ${minos:-unknown}"
  install_sidecar "$out" "$arch"
  built+=("$out")
done

if [ ${#built[@]} -gt 1 ]; then
  tmp="$(mktemp -d)"
  lipo -create -output "$tmp/lc0" "${built[@]}"
  install_sidecar "$tmp/lc0" universal
  rm -rf "$tmp"
fi
