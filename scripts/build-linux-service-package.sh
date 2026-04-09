#!/usr/bin/env bash
set -euo pipefail

CLEAN_DIST=true
SKIP_FRONTEND_BUILD=false
DRY_RUN=false
OUTPUT_DIR="target/release/linux-package"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --clean-dist)
      CLEAN_DIST=true
      shift
      ;;
    --no-clean-dist)
      CLEAN_DIST=false
      shift
      ;;
    --skip-frontend-build)
      SKIP_FRONTEND_BUILD=true
      shift
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      if [[ -z "$OUTPUT_DIR" ]]; then
        echo "--output-dir requires a value" >&2
        exit 2
      fi
      shift 2
      ;;
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FRONTEND_DIR="$ROOT/apps"
DIST_DIR="$FRONTEND_DIR/out"
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT/target}"
RELEASE_DIR="$TARGET_ROOT/release"

if [[ "$OUTPUT_DIR" = /* ]]; then
  OUTPUT_DIR_ABS="$OUTPUT_DIR"
else
  OUTPUT_DIR_ABS="$ROOT/$OUTPUT_DIR"
fi

step() {
  echo "$*"
}

run_cmd() {
  local display="$1"
  shift
  if [[ "$DRY_RUN" == "true" ]]; then
    step "DRY RUN: $display"
    return
  fi
  "$@"
}

remove_dir() {
  local path="$1"
  if [[ ! -e "$path" ]]; then
    return
  fi
  if [[ "$DRY_RUN" == "true" ]]; then
    step "DRY RUN: remove $path"
    return
  fi
  rm -rf "$path"
}

command -v cargo >/dev/null 2>&1 || { echo "cargo not found in PATH" >&2; exit 1; }
command -v install >/dev/null 2>&1 || { echo "install not found in PATH" >&2; exit 1; }
if [[ "$SKIP_FRONTEND_BUILD" != "true" ]]; then
  command -v pnpm >/dev/null 2>&1 || { echo "pnpm not found in PATH" >&2; exit 1; }
fi

if [[ "$CLEAN_DIST" == "true" ]]; then
  remove_dir "$DIST_DIR"
fi

if [[ "$SKIP_FRONTEND_BUILD" == "true" ]]; then
  step "skip frontend build; expecting existing dist at $DIST_DIR"
else
  run_cmd "pnpm --dir apps run build:desktop" pnpm --dir "$FRONTEND_DIR" run build:desktop
fi

if [[ "$DRY_RUN" != "true" ]] && [[ ! -f "$DIST_DIR/index.html" ]]; then
  echo "frontend dist missing: $DIST_DIR/index.html" >&2
  exit 1
fi

run_cmd \
  "cargo build -p codexmanager-service -p codexmanager-start --release" \
  cargo build -p codexmanager-service -p codexmanager-start --release
run_cmd \
  "cargo build -p codexmanager-web --release --features embedded-ui" \
  cargo build -p codexmanager-web --release --features embedded-ui

if [[ "$DRY_RUN" != "true" ]]; then
  if [[ ! -f "$RELEASE_DIR/codexmanager-service" ]]; then
    echo "binary not found: $RELEASE_DIR/codexmanager-service" >&2
    exit 1
  fi
  if [[ ! -f "$RELEASE_DIR/codexmanager-web" ]]; then
    echo "binary not found: $RELEASE_DIR/codexmanager-web" >&2
    exit 1
  fi
  if [[ ! -f "$RELEASE_DIR/codexmanager-start" ]]; then
    echo "binary not found: $RELEASE_DIR/codexmanager-start" >&2
    exit 1
  fi
fi

remove_dir "$OUTPUT_DIR_ABS"
run_cmd "install -d $OUTPUT_DIR_ABS" install -d "$OUTPUT_DIR_ABS"
run_cmd \
  "install -m 755 release binaries into $OUTPUT_DIR_ABS" \
  install -m 755 \
    "$RELEASE_DIR/codexmanager-service" \
    "$RELEASE_DIR/codexmanager-web" \
    "$RELEASE_DIR/codexmanager-start" \
    "$OUTPUT_DIR_ABS/"

step "linux service package ready:"
step "  $OUTPUT_DIR_ABS/codexmanager-service"
step "  $OUTPUT_DIR_ABS/codexmanager-web"
step "  $OUTPUT_DIR_ABS/codexmanager-start"
