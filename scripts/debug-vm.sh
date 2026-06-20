#!/usr/bin/env bash
# Open an interactive lldb session on the VM over SSH.
#
# Attaches to the same binary path used by deploy-vm.sh so breakpoints and symbols line up
# with a debug build deployed from the host.
#
# Typical lldb commands after launch:
#   run -- --config /opt/esgraph/config/default.toml
#   process attach --name esgraphd
#   bt
#
# Prerequisites: config/vm.env (see config/vm.env.example)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${ROOT}/config/vm.env"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "missing ${ENV_FILE} — copy config/vm.env.example and fill in VM host/user" >&2
  exit 1
fi

# Same connection settings as deploy-vm.sh.
# shellcheck disable=SC1090
source "${ENV_FILE}"

: "${ESGRAPH_VM_HOST:?set ESGRAPH_VM_HOST in config/vm.env}"
: "${ESGRAPH_VM_USER:?set ESGRAPH_VM_USER in config/vm.env}"
: "${ESGRAPH_INSTALL_PATH:=/opt/esgraph}"

REMOTE="${ESGRAPH_VM_USER}@${ESGRAPH_VM_HOST}"
BINARY="${ESGRAPH_INSTALL_PATH}/esgraphd"
CONFIG="${ESGRAPH_INSTALL_PATH}/config/default.toml"

echo "==> SSH → sudo lldb ${BINARY}"
echo "    suggested: run -- --config ${CONFIG}"
echo

# -t forces a TTY so lldb's interactive prompt works over SSH.
# sudo is required because live ESF collection runs as root; attaching to a root process
# also needs elevated privileges. lldb is launched with the deployed binary path only —
# use `run -- --config …` inside lldb to pass arguments to esgraphd.
exec ssh -t "${REMOTE}" "sudo lldb '${BINARY}'"
