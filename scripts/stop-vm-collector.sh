#!/usr/bin/env bash
# Stop any live esgraphd collector on the VM (esgraphd run).
#
# Note: `esgraphd status` is a one-shot DB read — it does not keep a collector running.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${ROOT}/config/vm.env"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "missing ${ENV_FILE}" >&2
  exit 1
fi

# shellcheck disable=SC1090
source "${ENV_FILE}"

: "${ESGRAPH_VM_HOST:?set ESGRAPH_VM_HOST in config/vm.env}"
: "${ESGRAPH_VM_USER:?set ESGRAPH_VM_USER in config/vm.env}"
: "${ESGRAPH_INSTALL_PATH:=/opt/esgraph}"

REMOTE="${ESGRAPH_VM_USER}@${ESGRAPH_VM_HOST}"
BINARY="${ESGRAPH_INSTALL_PATH}/esgraphd"
CONFIG_PATH="${ESGRAPH_INSTALL_PATH}/config/default.toml"

ssh -T "${REMOTE}" \
  "BINARY='${BINARY}' CONFIG_PATH='${CONFIG_PATH}' bash -s" <<'REMOTE'
set -euo pipefail

find_run_pids() {
  ps -ax -o pid=,command= 2>/dev/null \
    | awk -v bin="${BINARY}" '{
      pid = $1
      line = $0
      sub(/^[[:space:]]*[0-9]+[[:space:]]+/, "", line)
      if (index(line, bin " run ") == 1) print pid
    }'
}

signal_pid() {
  local pid="$1"
  local sig="$2"
  kill "-${sig}" "${pid}" 2>/dev/null \
    || sudo -n /bin/kill "-${sig}" "${pid}" 2>/dev/null
}

pid_alive() {
  local pid="$1"
  kill -0 "${pid}" 2>/dev/null || sudo -n /bin/kill -0 "${pid}" 2>/dev/null
}

pids="$(find_run_pids)"
if [[ -z "${pids}" ]]; then
  echo "no esgraphd run process found (collector is not running)"
  echo "tip: esgraphd status only reads the database; it is not the live collector"
  exit 0
fi

echo "found esgraphd run process(es):"
while IFS= read -r pid; do
  [[ -z "${pid}" ]] && continue
  ps -p "${pid}" -o pid=,command= 2>/dev/null || true
done <<< "${pids}"

while IFS= read -r pid; do
  [[ -z "${pid}" ]] && continue
  echo "==> stopping pid ${pid}"
  signal_pid "${pid}" INT || true
  sleep 2
  if pid_alive "${pid}"; then
    signal_pid "${pid}" TERM || true
    sleep 1
  fi
  if pid_alive "${pid}"; then
    signal_pid "${pid}" KILL || true
    sleep 1
  fi
done <<< "${pids}"

remaining="$(find_run_pids)"
if [[ -n "${remaining}" ]]; then
  echo "warning: collector still running:" >&2
  while IFS= read -r pid; do
    ps -p "${pid}" -o pid=,command= 2>/dev/null || true
  done <<< "${remaining}"
  exit 1
fi

echo "collector stopped"
REMOTE
