#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
#
# Clone and start an Ootle localnet using only the official swarm daemon commands
# from the tari-ootle README. Does not probe public networks.
#
# Usage:
#   scripts/localnet.sh clone
#   scripts/localnet.sh init
#   scripts/localnet.sh start
#   scripts/localnet.sh start-k
#
# Environment:
#   GHOSTLINE_OOTLE_DIR   parent of the `tari` and `ootle` clones (default: $HOME/dev/ootle-localnet)
#   GHOSTLINE_MIN_FREE_GB refuse compile/start below this many GiB free (default: 40)

set -euo pipefail

ROOT="${GHOSTLINE_OOTLE_DIR:-$HOME/dev/ootle-localnet}"
MIN_FREE_GB="${GHOSTLINE_MIN_FREE_GB:-40}"
TARI_REPO="https://github.com/tari-project/tari.git"
OOTLE_REPO="https://github.com/tari-project/tari-ootle.git"
SWARM_CONFIG="data/swarm/config.toml"

die() {
  echo "localnet: $*" >&2
  exit 1
}

require_disk() {
  local avail_kb avail_gb
  mkdir -p "$ROOT"
  avail_kb="$(df -Pk "$ROOT" | awk 'NR==2 {print $4}')"
  avail_gb=$((avail_kb / 1024 / 1024))
  if (( avail_gb < MIN_FREE_GB )); then
    die "${avail_gb}G free at $ROOT; a swarm compile needs tens of GB (floor is ${MIN_FREE_GB}G, override with GHOSTLINE_MIN_FREE_GB)"
  fi
}

l1_hint() {
  # Workspace pin in the Ootle tree, not the stale tag in the README.
  grep -E '^minotari_(app_grpc|node) ' "$ROOT/ootle/Cargo.toml" | head -n 2 || true
}

# hidapi (swarm daemon) needs libudev.pc. Prefer the distro package; otherwise a
# user-extracted copy under $HOME/.local/lib/pkgconfig (no sudo).
export_libudev_pkgconfig() {
  local user_pc="$HOME/.local/lib/pkgconfig"
  local udev_inc="$HOME/.local/opt/libudev-dev/usr/include"
  local udev_lib="$HOME/.local/lib"
  local udev_so="/usr/lib/x86_64-linux-gnu/libudev.so.1"
  if [[ -f "$user_pc/libudev.pc" ]]; then
    export PKG_CONFIG_PATH="${user_pc}${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  fi
  # hidapi's cc step compiles hid.c without pkg-config Cflags.
  if [[ -f "$udev_inc/libudev.h" ]]; then
    export CPATH="${udev_inc}${CPATH:+:$CPATH}"
    export CFLAGS="${CFLAGS:+$CFLAGS }-I${udev_inc}"
  fi
  # Distro libudev1 ships libudev.so.1 only; rust-lld needs the unversioned
  # libudev.so symlink from libudev-dev. Recreate it in a user prefix.
  if [[ -e "$udev_so" ]]; then
    mkdir -p "$udev_lib"
    ln -sfn "$udev_so" "$udev_lib/libudev.so"
    # gcc/collect2 honours LIBRARY_PATH; avoid RUSTFLAGS so cargo fingerprints stay valid.
    export LIBRARY_PATH="${udev_lib}${LIBRARY_PATH:+:$LIBRARY_PATH}"
    # Keep the last hidapi OUT_DIR on the native search path without a rebuild.
    local hid_out
    hid_out="$(find "$ROOT/ootle/target/release/build" -maxdepth 2 -type d -path '*/hidapi-*/out' 2>/dev/null | tail -n 1 || true)"
    if [[ -n "$hid_out" ]]; then
      ln -sfn "$udev_so" "$hid_out/libudev.so"
    fi
  fi
  if pkg-config --exists libudev 2>/dev/null && { echo '#include <libudev.h>' | cc ${CFLAGS:-} -E - >/dev/null 2>&1; }; then
    return 0
  fi
  die "libudev.pc / libudev.h missing (hidapi). Install libudev-dev, or extract headers under $udev_inc and $user_pc/libudev.pc"
}

cmd="${1:-}"
case "$cmd" in
  clone)
    require_disk
    mkdir -p "$ROOT"
    if [[ ! -d "$ROOT/tari/.git" ]]; then
      git clone --depth 1 "$TARI_REPO" "$ROOT/tari"
    fi
    if [[ ! -d "$ROOT/ootle/.git" ]]; then
      git clone --depth 1 "$OOTLE_REPO" "$ROOT/ootle"
    fi
    echo "Cloned under $ROOT. Check L1 against these Ootle pins, then checkout a matching tari tag:"
    l1_hint
    echo "Next: $0 init"
    ;;
  init)
    [[ -d "$ROOT/ootle" ]] || die "missing $ROOT/ootle — run: $0 clone"
    export_libudev_pkgconfig
    rustup target add wasm32-unknown-unknown
    export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
    (cd "$ROOT/ootle" && cargo run --bin tari_swarm_daemon --release -- -c "$SWARM_CONFIG" init)
    ;;
  start)
    require_disk
    export_libudev_pkgconfig
    [[ -f "$ROOT/ootle/$SWARM_CONFIG" ]] || die "missing swarm config — run: $0 init"
    echo "Swarm UI will be http://localhost:8080 — copy the indexer REST URL from there."
    export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
    (cd "$ROOT/ootle" && cargo run --bin tari_swarm_daemon --release -- -c "$SWARM_CONFIG" start)
    ;;
  start-k)
    require_disk
    export_libudev_pkgconfig
    [[ -f "$ROOT/ootle/$SWARM_CONFIG" ]] || die "missing swarm config — run: $0 init"
    (cd "$ROOT/ootle" && cargo run --bin tari_swarm_daemon --release -- -c "$SWARM_CONFIG" start -k)
    ;;
  *)
    cat <<EOF
Usage: $0 clone|init|start|start-k
  clone    fetch tari + ootle into GHOSTLINE_OOTLE_DIR ($ROOT)
  init     tari_swarm_daemon -c $SWARM_CONFIG init
  start    first start (builds binaries; needs tens of GB)
  start-k  subsequent start with --skip-registration (-k)
EOF
    exit 1
    ;;
esac
