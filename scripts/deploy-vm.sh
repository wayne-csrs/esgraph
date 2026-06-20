#!/usr/bin/env bash
# Build esgraphd on the host and rsync to the VM, then ad-hoc sign with ESF entitlement.
#
# Prerequisites:
#   - config/vm.env (copy from config/vm.env.example)
#   - SSH key login to the VM (see docs/vm-setup.md)
#   - Same CPU arch on host and guest (arm64 or x86_64)
#
# Usage:
#   ./scripts/deploy-vm.sh              # debug build (best for lldb)
#   ./scripts/deploy-vm.sh --release    # release build

set -euo pipefail

# Repo root — all paths below are relative to this directory.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${ROOT}/config/vm.env"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "missing ${ENV_FILE} — copy config/vm.env.example and fill in VM host/user" >&2
  exit 1
fi

# Load VM SSH target and install path (ESGRAPH_VM_HOST, ESGRAPH_VM_USER, …).
# shellcheck disable=SC1090
source "${ENV_FILE}"

# Fail fast if required variables are unset; default install path matches vm-setup.md.
: "${ESGRAPH_VM_HOST:?set ESGRAPH_VM_HOST in config/vm.env}"
: "${ESGRAPH_VM_USER:?set ESGRAPH_VM_USER in config/vm.env}"
: "${ESGRAPH_INSTALL_PATH:=/opt/esgraph}"

# Debug builds include symbols for lldb; release is smaller/faster but harder to debug.
PROFILE=debug
if [[ "${1:-}" == "--release" ]]; then
  PROFILE=release
fi

REMOTE="${ESGRAPH_VM_USER}@${ESGRAPH_VM_HOST}"
SSH=(ssh -tt "${REMOTE}")
# -a archive, -v verbose, -z compress; --delete removes stale remote files in synced dirs.
RSYNC=(rsync -avz --delete)

echo "==> building esgraphd (${PROFILE})"
(
  cd "${ROOT}"

  # Cross-compile is not used — the VM must match the host architecture.
  if [[ "${PROFILE}" == "release" ]]; then
    cargo build -p esgraphd --release
    BIN="${ROOT}/target/release/esgraphd"
  else
    cargo build -p esgraphd
    BIN="${ROOT}/target/debug/esgraphd"
  fi

  echo "==> preparing ${ESGRAPH_INSTALL_PATH} on VM"
  # /opt/esgraph is root-owned by default; chown to the deploy user so rsync can write.
  # Subdirs: config/ for TOML, data/ for LadybugDB file at runtime (created by esgraphd).
  "${SSH[@]}" "sudo mkdir -p '${ESGRAPH_INSTALL_PATH}/'{bin,config,data} && sudo chown -R '${ESGRAPH_VM_USER}' '${ESGRAPH_INSTALL_PATH}'"

  echo "==> rsync binary + config"
  # Binary lands at a fixed path so FDA rules and lldb commands stay consistent.
  "${RSYNC[@]}" "${BIN}" "${REMOTE}:${ESGRAPH_INSTALL_PATH}/esgraphd"
  # VM-specific TOML (absolute DB path under /opt/esgraph/data).
  "${RSYNC[@]}" "${ROOT}/config/vm.default.toml" "${REMOTE}:${ESGRAPH_INSTALL_PATH}/config/default.toml"
  # Entitlements plist is required for codesign on the VM (not embedded in the binary until signed).
  "${RSYNC[@]}" "${ROOT}/esgraphd.entitlements" "${REMOTE}:${ESGRAPH_INSTALL_PATH}/esgraphd.entitlements"

  echo "==> codesign on VM (ad-hoc + Endpoint Security entitlement)"
  # Signing must happen on the VM (or with a cert trusted there). Ad-hoc (-) is enough for a
  # dedicated guest with SIP/AMFI relaxed. --options runtime enables hardened runtime flags.
  # Without com.apple.developer.endpoint-security.client, es_new_client returns NOT_ENTITLED.
  "${SSH[@]}" "codesign --force --sign - \
    --entitlements '${ESGRAPH_INSTALL_PATH}/esgraphd.entitlements' \
    --options runtime \
    '${ESGRAPH_INSTALL_PATH}/esgraphd'"

  echo "==> verify signature"
  # Print entitlements blob so you can confirm the ES client key is present before first run.
  "${SSH[@]}" "codesign -dv --entitlements - '${ESGRAPH_INSTALL_PATH}/esgraphd' 2>&1 | head -20"

  echo
  echo "deployed to ${REMOTE}:${ESGRAPH_INSTALL_PATH}/esgraphd"
  echo "run on VM:"
  echo "  sudo ${ESGRAPH_INSTALL_PATH}/esgraphd run --config ${ESGRAPH_INSTALL_PATH}/config/default.toml"
)
