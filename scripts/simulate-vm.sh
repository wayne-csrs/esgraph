#!/usr/bin/env bash
# Run a complete VM simulation workflow:
# 1) reset collector state on the VM (no ESF)
# 2) start esgraphd live ESF collection on the VM
# 3) run scenario commands one-by-one (or a single ad-hoc command)
# 4) stop esgraphd cleanly (end ESF before any post-run management)
# 5) collect status + archive database on the VM (no ESF)
# 6) copy logs + LadybugDB database back to host artefacts directory
# 7) run scenario cleanup script on the VM (named scenarios only)
# 8) remove copied artefacts from the VM

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${ROOT}/config/vm.env"
SCENARIOS_DIR="${ROOT}/scripts/attack-scenarios"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "missing ${ENV_FILE} — copy config/vm.env.example and fill in VM host/user" >&2
  exit 1
fi
if [[ ! -d "${SCENARIOS_DIR}" ]]; then
  echo "missing ${SCENARIOS_DIR}" >&2
  exit 1
fi

# shellcheck disable=SC1090
source "${ENV_FILE}"

: "${ESGRAPH_VM_HOST:?set ESGRAPH_VM_HOST in config/vm.env}"
: "${ESGRAPH_VM_USER:?set ESGRAPH_VM_USER in config/vm.env}"
: "${ESGRAPH_INSTALL_PATH:=/opt/esgraph}"

WARMUP_SEC=3
COOLDOWN_SEC=3
COMMAND_DELAY_SEC=1
RUST_LOG_LEVEL="esgraph=info"
CONFIG_PATH="${ESGRAPH_INSTALL_PATH}/config/default.toml"
OUTPUT_DIR="${ROOT}/artefacts/simulations"
SCENARIO_NAME=""
SCENARIO_FILE=""
CLEANUP_FILE=""
HOST_HTTP=""
HOST_HTTP_PORT="${ESGRAPH_HOST_HTTP_PORT:-8765}"
ESGRAPH_HOST_HTTP=""
SCENARIO_HTTP_DIR=""

scenario_wants_host_http() {
  local file="$1"
  awk -F': ' '
    /^# host-http:/ {
      v = tolower($2)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      if (v ~ /^(yes|true|1|required|auto)$/) {
        print "yes"
        exit
      }
    }
  ' "${file}"
}

resolve_scenario_files() {
  local name="$1"
  local candidate rel cleanup
  while IFS= read -r candidate; do
    rel="${candidate#${SCENARIOS_DIR}/}"
    rel="${rel%.scenario.sh}"
    if [[ "${rel}" == "${name}" ]]; then
      cleanup="${candidate%.scenario.sh}.cleanup.sh"
      if [[ -f "${cleanup}" ]]; then
        SCENARIO_FILE="${candidate}"
        CLEANUP_FILE="${cleanup}"
        return 0
      fi
      break
    fi
  done < <(scenario_files)
  return 1
}

scenario_files() {
  list_scenarios_in_dir "${SCENARIOS_DIR}"
}

list_scenarios_in_dir() {
  local dir="$1"
  local entry
  shopt -s nullglob
  for entry in "${dir}"/*; do
    if [[ -d "${entry}" ]]; then
      list_scenarios_in_dir "${entry}"
    elif [[ "${entry}" == *.scenario.sh ]]; then
      printf '%s\n' "${entry}"
    fi
  done
  shopt -u nullglob
}

parse_scenario_commands() {
  local file="$1"
  local block=""
  local in_heredoc=0
  local heredoc_marker=""
  while IFS= read -r line || [[ -n "${line}" ]]; do
    if [[ "${line}" =~ ^[[:space:]]*# ]]; then
      continue
    fi
    if [[ -z "${line//[[:space:]]/}" ]]; then
      continue
    fi

    if [[ "${in_heredoc}" == "1" ]]; then
      block+=$'\n'"${line}"
      if [[ "${line}" == "${heredoc_marker}" ]]; then
        printf '%s' "${block}" | base64 | tr -d '\n'
        printf '\n'
        block=""
        in_heredoc=0
        heredoc_marker=""
      fi
      continue
    fi

    block="${line}"
    if [[ "${line}" =~ \<\<[\'\"]?([A-Za-z0-9_]+) ]]; then
      in_heredoc=1
      heredoc_marker="${BASH_REMATCH[1]}"
      continue
    fi

    printf '%s' "${block}" | base64 | tr -d '\n'
    printf '\n'
    block=""
  done < "${file}"
  if [[ -n "${block}" ]]; then
    printf '%s' "${block}" | base64 | tr -d '\n'
    printf '\n'
  fi
}

list_scenarios() {
  local file name desc mitre cleanup
  while IFS= read -r file; do
    cleanup="${file%.scenario.sh}.cleanup.sh"
    if [[ ! -f "${cleanup}" ]]; then
      continue
    fi
    name="${file#${SCENARIOS_DIR}/}"
    name="${name%.scenario.sh}"
    desc="$(awk -F': ' '/^# description:/{print $2; exit}' "${file}")"
    mitre="$(awk -F': ' '/^# mitre:/{print $2; exit}' "${file}")"
    if [[ -z "${desc}" ]]; then
      desc="(no description)"
    fi
    if [[ -n "${mitre}" ]]; then
      printf '%-24s | %s\n' "${name}" "${desc}"
      printf '%-24s | MITRE: %s\n' "" "${mitre}"
    else
      printf '%-24s | %s\n' "${name}" "${desc}"
    fi
  done < <(scenario_files)
}

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/simulate-vm.sh [options] --scenario <name>
  ./scripts/simulate-vm.sh [options] -- <attack command> [args...]

Options:
  --scenario NAME      Scenario name from scripts/attack-scenarios/ (e.g. apt29/apt29_discovery)
  --list-scenarios     Print available scenarios and exit
  --warmup-sec N       Seconds to wait after starting esgraphd (default: 3)
  --cooldown-sec N     Seconds to wait after the last attack command (default: 3)
  --command-delay-sec N  Seconds to wait between scenario commands (default: 1)
  --rust-log FILTER    RUST_LOG value for esgraphd on VM (default: esgraph=info)
  --config PATH        Config path on VM (default: /opt/esgraph/config/default.toml)
  --output-dir PATH    Host artefacts directory (default: ./artefacts/simulations)
  --host-http          Force-start the host HTTP server (also auto-started for # host-http: scenarios)
  --no-host-http       Do not start the host HTTP server, even for # host-http: scenarios
  --host-http-port N   Port for the host HTTP server (default: 8765 or ESGRAPH_HOST_HTTP_PORT)
  -h, --help           Show this help

Examples:
  ./scripts/simulate-vm.sh --scenario apt29/apt29_discovery
  ./scripts/simulate-vm.sh --scenario amos/amos_stealer_chain
  ./scripts/simulate-vm.sh -- touch /tmp/esgraph-test
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --scenario)
      SCENARIO_NAME="${2:?missing value for --scenario}"
      shift 2
      ;;
    --list-scenarios)
      list_scenarios
      exit 0
      ;;
    --warmup-sec)
      WARMUP_SEC="${2:?missing value for --warmup-sec}"
      shift 2
      ;;
    --cooldown-sec)
      COOLDOWN_SEC="${2:?missing value for --cooldown-sec}"
      shift 2
      ;;
    --command-delay-sec)
      COMMAND_DELAY_SEC="${2:?missing value for --command-delay-sec}"
      shift 2
      ;;
    --rust-log)
      RUST_LOG_LEVEL="${2:?missing value for --rust-log}"
      shift 2
      ;;
    --config)
      CONFIG_PATH="${2:?missing value for --config}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:?missing value for --output-dir}"
      shift 2
      ;;
    --host-http)
      HOST_HTTP=1
      shift
      ;;
    --no-host-http)
      HOST_HTTP=0
      shift
      ;;
    --host-http-port)
      HOST_HTTP_PORT="${2:?missing value for --host-http-port}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -n "${SCENARIO_NAME}" && $# -gt 0 ]]; then
  echo "choose one input mode: either --scenario or explicit command after --" >&2
  exit 2
fi
if [[ -z "${SCENARIO_NAME}" && $# -eq 0 ]]; then
  echo "missing attack command (use --scenario NAME or -- <command ...>)" >&2
  exit 2
fi

if [[ -n "${SCENARIO_NAME}" ]]; then
  if ! resolve_scenario_files "${SCENARIO_NAME}"; then
    echo "unknown scenario: ${SCENARIO_NAME}" >&2
    echo
    echo "available scenarios:"
    list_scenarios
    exit 2
  fi
  ATTACK_PAYLOAD="$(parse_scenario_commands "${SCENARIO_FILE}")"
  if [[ -z "${ATTACK_PAYLOAD//[[:space:]]/}" ]]; then
    echo "scenario has no executable commands: ${SCENARIO_FILE}" >&2
    exit 2
  fi
  CLEANUP_SCRIPT="$(awk '!/^#/' "${CLEANUP_FILE}")"
  ATTACK_B64="$(printf '%s' "${ATTACK_PAYLOAD}" | base64 | tr -d '\n')"
  CLEANUP_B64="$(printf '%s' "${CLEANUP_SCRIPT}" | base64 | tr -d '\n')"
  ATTACK_MODE="scenario"
  if [[ -z "${HOST_HTTP}" && "$(scenario_wants_host_http "${SCENARIO_FILE}")" == "yes" ]]; then
    HOST_HTTP=1
    echo "==> scenario ${SCENARIO_NAME} declares # host-http: yes; starting host HTTP server"
  fi
  if [[ "${HOST_HTTP}" == "0" && "$(scenario_wants_host_http "${SCENARIO_FILE}")" == "yes" ]]; then
    echo "warning: ${SCENARIO_NAME} declares # host-http: yes but --no-host-http was passed; payload curls will fail" >&2
  fi
else
  ADHOC_CMD="$(printf '%q ' "$@")"
  ATTACK_B64="$(printf '%s' "${ADHOC_CMD}" | base64 | tr -d '\n')"
  ATTACK_MODE="adhoc"
fi

REMOTE="${ESGRAPH_VM_USER}@${ESGRAPH_VM_HOST}"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
LABEL="${SCENARIO_NAME:-custom}"
LABEL_SAFE="$(printf '%s' "${LABEL}" | tr -c 'A-Za-z0-9._-' '_')"
HOST_RUN_DIR="${OUTPUT_DIR}/${RUN_ID}-${LABEL_SAFE}"

mkdir -p "${HOST_RUN_DIR}"

start_host_http() {
  SCENARIO_HTTP_DIR="${HOST_RUN_DIR}/host-http"
  echo "==> starting host HTTP server on port ${HOST_HTTP_PORT}"
  ESGRAPH_HOST_HTTP_PORT="${HOST_HTTP_PORT}" \
    "${ROOT}/scripts/scenario-http-server.sh" start --root "${SCENARIO_HTTP_DIR}" --port "${HOST_HTTP_PORT}"
  if [[ ! -f "${SCENARIO_HTTP_DIR}/host-http.url" ]]; then
    echo "failed to start host HTTP server" >&2
    exit 1
  fi
  ESGRAPH_HOST_HTTP="$(tr -d '\r\n' < "${SCENARIO_HTTP_DIR}/host-http.url")"
  echo "    ESGRAPH_HOST_HTTP=${ESGRAPH_HOST_HTTP}"
}

stop_host_http() {
  if [[ "${HOST_HTTP}" != "1" || -z "${SCENARIO_HTTP_DIR}" ]]; then
    return 0
  fi
  echo "==> stopping host HTTP server"
  "${ROOT}/scripts/scenario-http-server.sh" stop --root "${SCENARIO_HTTP_DIR}" || true
}

if [[ "${HOST_HTTP}" == "1" ]]; then
  start_host_http
fi

if [[ -n "${SCENARIO_NAME}" ]]; then
  {
    echo "# scenario: ${SCENARIO_NAME}"
    while IFS= read -r cmd_b64; do
      [[ -z "${cmd_b64}" ]] && continue
      printf '%s' "${cmd_b64}" | base64 -d
      printf '\n\n'
    done <<< "${ATTACK_PAYLOAD}"
  } > "${HOST_RUN_DIR}/attack-command.txt"
else
  printf '%s\n' "${ADHOC_CMD}" > "${HOST_RUN_DIR}/attack-command.txt"
fi

SSH_OUTPUT="$(mktemp)"
SCENARIO_CLEANUP_DONE=0
SIM_PHASE="init"
CLEANUP_EXIT=""
CLEANUP_VERIFY=""
CLEANUP_REMAINING=""

cleanup_tmp() { rm -f "${SSH_OUTPUT}"; }

stop_remote_collector() {
  echo "==> stopping remote esgraphd before post-run management"
  set +e
  ssh -T "${REMOTE}" \
    "ESGRAPH_INSTALL_PATH='${ESGRAPH_INSTALL_PATH}' bash -s" <<'EOF'
set -euo pipefail
BINARY="${ESGRAPH_INSTALL_PATH}/esgraphd"
find_esgraphd_pids() {
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
while IFS= read -r pid; do
  [[ -z "${pid}" ]] && continue
  signal_pid "${pid}" INT || true
done < <(find_esgraphd_pids)
EOF
  set -e
}

parse_cleanup_removes() {
  local file="$1"
  awk -F': ' '/^# removes:/{print $2}' "${file}"
}

expand_cleanup_path() {
  local template="$1"
  template="${template//\$\{ESGRAPH_RUN_ID\}/${RUN_ID}}"
  printf '%s' "${template}"
}

build_cleanup_verify_paths() {
  local file="$1"
  local line part
  while IFS= read -r line; do
    [[ -z "${line}" ]] && continue
    IFS=',' read -ra parts <<< "${line}"
    for part in "${parts[@]}"; do
      part="${part#"${part%%[![:space:]]*}"}"
      part="${part%"${part##*[![:space:]]}"}"
      [[ "${part}" == /* ]] || continue
      expand_cleanup_path "${part}"
    done
  done < <(parse_cleanup_removes "${file}")
}

run_scenario_cleanup() {
  if [[ -z "${SCENARIO_NAME}" || "${SCENARIO_CLEANUP_DONE}" == "1" ]]; then
    return 0
  fi
  if [[ "${SIM_PHASE}" == "collecting" ]]; then
    stop_remote_collector
  fi
  SCENARIO_CLEANUP_DONE=1
  CLEANUP_EXIT=0
  CLEANUP_VERIFY="skipped"
  CLEANUP_REMAINING=""

  echo "==> running scenario cleanup on VM (ESF collection stopped)"
  set +e
  ssh -T "${REMOTE}" \
    "ESGRAPH_RUN_ID='${RUN_ID}' ESGRAPH_SCENARIO_NAME='${SCENARIO_NAME}' CLEANUP_B64='${CLEANUP_B64}' bash -s" <<'EOF'
set -euo pipefail
export ESGRAPH_RUN_ID="${ESGRAPH_RUN_ID}"
CLEANUP_SCRIPT="$(printf '%s' "${CLEANUP_B64}" | base64 -d)"
bash -c "${CLEANUP_SCRIPT}"
EOF
  CLEANUP_EXIT=$?
  set -e
  if [[ "${CLEANUP_EXIT}" -ne 0 ]]; then
    echo "warning: scenario cleanup script exited with status ${CLEANUP_EXIT}" >&2
  fi

  local verify_paths verify_paths_b64 verify_output verify_exit line path
  verify_paths="$(build_cleanup_verify_paths "${CLEANUP_FILE}")"
  if [[ -z "${verify_paths//[[:space:]]/}" ]]; then
    echo "    cleanup verify: skipped (no # removes: paths declared in cleanup script)"
    return 0
  fi

  verify_paths_b64="$(printf '%s' "${verify_paths}" | base64 | tr -d '\n')"
  echo "==> verifying scenario cleanup on VM"
  set +e
  verify_output="$(ssh -T "${REMOTE}" \
    "PATHS_B64='${verify_paths_b64}' bash -s" <<'EOF'
set -euo pipefail
fail=0
while IFS= read -r path; do
  [[ -z "${path}" ]] && continue
  if [[ -e "${path}" ]]; then
    printf 'REMAIN:%s\n' "${path}"
    fail=1
  fi
done < <(printf '%s' "${PATHS_B64}" | base64 -d)
exit "${fail}"
EOF
)"
  verify_exit=$?
  set -e

  local -a remaining=()
  while IFS= read -r line; do
    if [[ "${line}" == REMAIN:* ]]; then
      remaining+=("${line#REMAIN:}")
    fi
  done <<< "${verify_output}"

  if [[ "${verify_exit}" -eq 0 ]]; then
    CLEANUP_VERIFY="ok"
    echo "    scenario cleanup: successful (all declared artefacts removed)"
  else
    CLEANUP_VERIFY="failed"
    CLEANUP_REMAINING="$(printf '%s;' "${remaining[@]}")"
    CLEANUP_REMAINING="${CLEANUP_REMAINING%;}"
    echo "    scenario cleanup: failed — the following paths still exist on the VM:" >&2
    for path in "${remaining[@]}"; do
      echo "      - ${path}" >&2
    done
  fi
}

on_host_signal() {
  if [[ "${SIM_PHASE}" == "collecting" ]]; then
    stop_remote_collector || true
  fi
  if [[ "${SIM_PHASE}" != "init" && "${SIM_PHASE}" != "done" ]]; then
    run_scenario_cleanup
  fi
  stop_host_http
  cleanup_tmp
  exit 130
}

on_host_exit() {
  if [[ -n "${SCENARIO_NAME}" && "${SCENARIO_CLEANUP_DONE}" != "1" && "${SIM_PHASE}" != "init" && "${SIM_PHASE}" != "done" ]]; then
    run_scenario_cleanup
  fi
  stop_host_http
  cleanup_tmp
}

trap on_host_signal INT TERM
trap on_host_exit EXIT

set +e
SIM_PHASE="collecting"
ssh -T "${REMOTE}" \
  "ESGRAPH_INSTALL_PATH='${ESGRAPH_INSTALL_PATH}' CONFIG_PATH='${CONFIG_PATH}' RUST_LOG_LEVEL='${RUST_LOG_LEVEL}' WARMUP_SEC='${WARMUP_SEC}' COOLDOWN_SEC='${COOLDOWN_SEC}' COMMAND_DELAY_SEC='${COMMAND_DELAY_SEC}' RUN_ID='${RUN_ID}' ESGRAPH_RUN_ID='${RUN_ID}' ESGRAPH_HOST_HTTP='${ESGRAPH_HOST_HTTP}' ATTACK_MODE='${ATTACK_MODE}' ATTACK_B64='${ATTACK_B64}' bash -s" \
  <<'REMOTE_SCRIPT' | tee "${SSH_OUTPUT}"
set -euo pipefail

ATTACK_PAYLOAD="$(printf '%s' "${ATTACK_B64}" | base64 -d)"
BINARY="${ESGRAPH_INSTALL_PATH}/esgraphd"
SIM_DIR="/tmp/esgraph-sim-${RUN_ID}"
RUN_LOG="${SIM_DIR}/esgraphd-run.log"
STATUS_LOG="${SIM_DIR}/status.txt"
GRAPH_ARCHIVE="${SIM_DIR}/events.lbug.tar.gz"
export ESGRAPH_RUN_ID="${RUN_ID}"
export ESGRAPH_HOST_HTTP="${ESGRAPH_HOST_HTTP:-}"

mkdir -p "${SIM_DIR}"
if [[ -n "${ESGRAPH_HOST_HTTP}" ]]; then
  echo "    ESGRAPH_HOST_HTTP=${ESGRAPH_HOST_HTTP}"
fi

if [[ ! -x "${BINARY}" ]]; then
  echo "missing executable ${BINARY}. Run scripts/deploy-vm.sh first." >&2
  exit 1
fi

if ! sudo -n /usr/bin/env true 2>/dev/null; then
  echo "passwordless sudo is required for automated simulation on VM." >&2
  echo "configure sudoers for ${USER} (see docs/vm-setup.md)." >&2
  exit 1
fi
if ! sudo -n /bin/kill -l >/dev/null 2>&1; then
  echo "passwordless sudo is required for /bin/kill (see docs/vm-setup.md)." >&2
  exit 1
fi

signal_pid() {
  local pid="$1"
  local sig="$2"
  kill "-${sig}" "${pid}" 2>/dev/null \
    || sudo -n /bin/kill "-${sig}" "${pid}" 2>/dev/null
}

pid_alive() {
  local pid="$1"
  [[ -n "${pid}" ]] && ps -p "${pid}" -o pid= >/dev/null 2>&1
}

find_esgraphd_pids() {
  ps -ax -o pid=,command= 2>/dev/null \
    | awk -v bin="${BINARY}" '{
      pid = $1
      line = $0
      sub(/^[[:space:]]*[0-9]+[[:space:]]+/, "", line)
      if (index(line, bin " run ") == 1) print pid
    }'
}

stop_pid() {
  local pid="$1"
  local tries=0

  [[ -z "${pid}" ]] && return 0
  echo "    signalling pid ${pid}"
  signal_pid "${pid}" INT || true

  while pid_alive "${pid}"; do
    if [[ "${tries}" -ge 10 ]]; then
      signal_pid "${pid}" TERM || true
      sleep 1
    fi
    if [[ "${tries}" -ge 14 ]]; then
      signal_pid "${pid}" KILL || true
      sleep 1
      break
    fi
    sleep 0.5
    tries=$((tries + 1))
  done
}

stop_all_esgraphd() {
  local pid
  local found=0
  while IFS= read -r pid; do
    [[ -z "${pid}" ]] && continue
    found=1
    stop_pid "${pid}"
  done < <(find_esgraphd_pids)
  if [[ "${found}" == "1" ]]; then
    sleep 0.5
  fi
}

wait_pid() {
  local pid="$1"
  local tries=0
  while pid_alive "${pid}"; do
    [[ "${tries}" -ge 10 ]] && return 0
    sleep 0.5
    tries=$((tries + 1))
  done
  wait "${pid}" 2>/dev/null || true
}

stop_esgraphd() {
  local sudo_pid="$1"

  if [[ -n "${ESGRAPHD_PID:-}" ]]; then
    stop_pid "${ESGRAPHD_PID}"
  fi
  stop_all_esgraphd

  if [[ -n "${sudo_pid}" ]]; then
    signal_pid "${sudo_pid}" INT 2>/dev/null || true
    wait_pid "${sudo_pid}"
  fi

  if [[ -n "$(find_esgraphd_pids)" ]]; then
    echo "warning: esgraphd run still present after stop; run ./scripts/stop-vm-collector.sh" >&2
  fi
}

cleanup() {
  if [[ -n "${ESGRAPH_PID:-}" || -n "${ESGRAPHD_PID:-}" ]]; then
    stop_esgraphd "${ESGRAPH_PID:-}" || true
  fi
}
trap cleanup EXIT INT TERM

echo "==> pre-collection setup (ESF off)"
echo "    clearing stale collectors (if any)"
stop_all_esgraphd

store_path_from_config() {
  awk -F= '
    /^[[:space:]]*path[[:space:]]*=/ {
      v = $2
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      gsub(/^"/, "", v)
      gsub(/"$/, "", v)
      print v
      exit
    }
  ' "${CONFIG_PATH}"
}

echo "==> resetting LadybugDB for this simulation run"
DB_PATH="$(store_path_from_config)"
if [[ -n "${DB_PATH}" ]]; then
  echo "    removing ${DB_PATH} and ${DB_PATH}.wal (if present)"
  sudo -n rm -f "${DB_PATH}" "${DB_PATH}.wal"
else
  echo "warning: could not read store.path from ${CONFIG_PATH}" >&2
fi

echo "==> starting ESF collection (esgraphd on VM)"
sudo -n env RUST_LOG="${RUST_LOG_LEVEL}" "${BINARY}" run --config "${CONFIG_PATH}" >"${RUN_LOG}" 2>&1 &
ESGRAPH_PID=$!
ESGRAPHD_PID=""
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
  _child_pids="$(pgrep -P "${ESGRAPH_PID}" 2>/dev/null || true)"
  if [[ -n "${_child_pids}" ]]; then
    ESGRAPHD_PID="$(printf '%s\n' "${_child_pids}" | head -1)"
  fi
  if [[ -z "${ESGRAPHD_PID}" ]]; then
    ESGRAPHD_PID="$(ps -ax -o pid=,ppid=,command= 2>/dev/null \
      | awk -v ppid="${ESGRAPH_PID}" -v bin="${BINARY}" \
        '$2 == ppid {
          line = $0
          sub(/^[[:space:]]*[0-9]+[[:space:]]+[0-9]+[[:space:]]+/, "", line)
          if (index(line, bin " run ") == 1) { print $1; exit }
        }')"
  fi
  if [[ -n "${ESGRAPHD_PID}" ]]; then
    break
  fi
  sleep 0.2
done
if [[ -z "${ESGRAPHD_PID}" ]]; then
  ESGRAPHD_PID="$(find_esgraphd_pids | head -1 || true)"
fi

if [[ -z "${ESGRAPHD_PID}" ]] || ! pid_alive "${ESGRAPHD_PID}"; then
  echo "failed to start esgraphd; recent log output:" >&2
  tail -30 "${RUN_LOG}" 2>/dev/null >&2 || true
  exit 1
fi
echo "    sudo pid: ${ESGRAPH_PID}"
echo "    esgraphd pid: ${ESGRAPHD_PID:-unknown}"
echo "    log: ${RUN_LOG}"
echo "==> warmup ${WARMUP_SEC}s"
sleep "${WARMUP_SEC}"

echo "==> running simulation commands"
ATTACK_EXIT=0
if [[ "${ATTACK_MODE}" == "adhoc" ]]; then
  ADHOC_CMD="$(printf '%s' "${ATTACK_PAYLOAD}")"
  printf '    %s\n' "${ADHOC_CMD}"
  set +e
  eval "${ADHOC_CMD}"
  ATTACK_EXIT=$?
  set -e
else
  step=0
  while IFS= read -r cmd_b64; do
    [[ -z "${cmd_b64}" ]] && continue
    step=$((step + 1))
    cmd="$(printf '%s' "${cmd_b64}" | base64 -d)"
    printf '    [%s] %s\n' "${step}" "${cmd%%$'\n'*}"
    set +e
    if [[ "${cmd}" == *$'\n'* || "${cmd}" == *"<<"* ]]; then
      step_script="${SIM_DIR}/attack-step-${step}.sh"
      printf '%s\n' "${cmd}" > "${step_script}"
      bash "${step_script}"
      step_exit=$?
    else
      eval "${cmd}"
      step_exit=$?
    fi
    set -e
    if [[ "${step_exit}" -ne 0 ]]; then
      ATTACK_EXIT="${step_exit}"
    fi
    sleep "${COMMAND_DELAY_SEC}"
  done <<< "${ATTACK_PAYLOAD}"
fi

echo "==> cooldown ${COOLDOWN_SEC}s"
sleep "${COOLDOWN_SEC}"

echo "==> stopping ESF collection (esgraphd)"
stop_esgraphd "${ESGRAPH_PID}"
ESGRAPH_PID=""
ESGRAPHD_PID=""

echo "==> post-collection management (ESF off)"
echo "    collecting status"
if ! sudo -n "${BINARY}" status --config "${CONFIG_PATH}" | tee "${STATUS_LOG}"; then
  echo "failed to run status without interactive sudo; check sudo timeout/policy" >&2
  exit 1
fi
GRAPH_PATH="$(awk '/^graph:/{print $2}' "${STATUS_LOG}" | tail -1)"
if [[ -z "${GRAPH_PATH}" ]]; then
  GRAPH_PATH="$(awk '/^database:/{print $2}' "${STATUS_LOG}" | tail -1)"
fi
if [[ -n "${GRAPH_PATH}" && -f "${GRAPH_PATH}" ]]; then
  WAL_PATH="${GRAPH_PATH}.wal"
  TAR_ITEMS=("$(basename "${GRAPH_PATH}")")
  if [[ -f "${WAL_PATH}" ]]; then
    TAR_ITEMS+=("$(basename "${WAL_PATH}")")
  fi
  if ! sudo -n tar -czf "${GRAPH_ARCHIVE}" -C "$(dirname "${GRAPH_PATH}")" "${TAR_ITEMS[@]}"; then
    echo "failed to archive database at ${GRAPH_PATH}; add tar to sudoers (see docs/vm-setup.md)" >&2
    exit 1
  fi
fi

echo "RESULT_ATTACK_EXIT=${ATTACK_EXIT}"
echo "RESULT_RUN_LOG=${RUN_LOG}"
echo "RESULT_STATUS_LOG=${STATUS_LOG}"
echo "RESULT_GRAPH_ARCHIVE=${GRAPH_ARCHIVE}"
echo "RESULT_GRAPH_SOURCE=${GRAPH_PATH}"

exit "${ATTACK_EXIT}"
REMOTE_SCRIPT
SSH_EXIT=$?
set -e

SSH_TEXT="$(tr -d '\r' < "${SSH_OUTPUT}")"
ATTACK_EXIT="$(printf '%s\n' "${SSH_TEXT}" | awk -F= '/^RESULT_ATTACK_EXIT=/{print $2}' | tail -1)"
RUN_LOG_REMOTE="$(printf '%s\n' "${SSH_TEXT}" | awk -F= '/^RESULT_RUN_LOG=/{print $2}' | tail -1)"
STATUS_LOG_REMOTE="$(printf '%s\n' "${SSH_TEXT}" | awk -F= '/^RESULT_STATUS_LOG=/{print $2}' | tail -1)"
GRAPH_ARCHIVE_REMOTE="$(printf '%s\n' "${SSH_TEXT}" | awk -F= '/^RESULT_GRAPH_ARCHIVE=/{print $2}' | tail -1)"

if [[ -z "${RUN_LOG_REMOTE}" || -z "${STATUS_LOG_REMOTE}" || -z "${GRAPH_ARCHIVE_REMOTE}" ]]; then
  echo "failed to parse remote result paths; see output above" >&2
  exit "${SSH_EXIT:-1}"
fi

SIM_PHASE="copying"
echo "==> copying artefacts to host: ${HOST_RUN_DIR}"
scp "${REMOTE}:${RUN_LOG_REMOTE}" "${HOST_RUN_DIR}/esgraphd-run.log"
scp "${REMOTE}:${STATUS_LOG_REMOTE}" "${HOST_RUN_DIR}/status.txt"
scp "${REMOTE}:${GRAPH_ARCHIVE_REMOTE}" "${HOST_RUN_DIR}/events.lbug.tar.gz"
if [[ -f "${HOST_RUN_DIR}/events.lbug.tar.gz" ]]; then
  tar -xzf "${HOST_RUN_DIR}/events.lbug.tar.gz" -C "${HOST_RUN_DIR}"
fi

cat > "${HOST_RUN_DIR}/run-meta.txt" <<EOF2
run_id=${RUN_ID}
scenario=${LABEL}
remote=${REMOTE}
config_path=${CONFIG_PATH}
warmup_sec=${WARMUP_SEC}
cooldown_sec=${COOLDOWN_SEC}
command_delay_sec=${COMMAND_DELAY_SEC}
rust_log=${RUST_LOG_LEVEL}
attack_exit=${ATTACK_EXIT:-unknown}
remote_run_log=${RUN_LOG_REMOTE}
remote_status_log=${STATUS_LOG_REMOTE}
remote_graph_archive=${GRAPH_ARCHIVE_REMOTE}
host_http=${ESGRAPH_HOST_HTTP:-}
host_http_uploads=${SCENARIO_HTTP_DIR:+$SCENARIO_HTTP_DIR/uploads}
cleanup_exit=${CLEANUP_EXIT:-}
cleanup_verify=${CLEANUP_VERIFY:-}
cleanup_remaining=${CLEANUP_REMAINING:-}
EOF2

run_scenario_cleanup

echo "==> removing copied artefacts from VM"
ssh -T "${REMOTE}" "rm -rf '/tmp/esgraph-sim-${RUN_ID}'" || \
  echo "warning: could not remove remote simulation artefacts"

SIM_PHASE="done"

echo
echo "simulation completed"
echo "host artefacts: ${HOST_RUN_DIR}"
echo "  - esgraphd-run.log"
echo "  - status.txt"
echo "  - events.lbug.tar.gz"
echo "  - events.lbug (+ events.lbug.wal if present)"
echo "  - run-meta.txt"
echo "  - attack-command.txt"
if [[ -n "${SCENARIO_NAME}" ]]; then
  if [[ "${CLEANUP_VERIFY}" == "ok" ]]; then
    echo "scenario cleanup: successful"
  elif [[ "${CLEANUP_VERIFY}" == "failed" ]]; then
    echo "scenario cleanup: failed (see warnings above)"
  elif [[ "${CLEANUP_VERIFY}" == "skipped" ]]; then
    echo "scenario cleanup: not verified (no # removes: paths declared)"
  fi
fi

if [[ -n "${ATTACK_EXIT:-}" ]]; then
  exit "${ATTACK_EXIT}"
fi
exit "${SSH_EXIT}"
