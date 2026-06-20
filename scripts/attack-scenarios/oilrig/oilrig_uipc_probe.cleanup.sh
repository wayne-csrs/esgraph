# description: Remove OilRig probe artefacts from the VM
# removes: /tmp/oilrig_chain_${ESGRAPH_RUN_ID}, /tmp/esgraph-oilrig-uipc.sock, /private/tmp/esgraph-oilrig-uipc.sock

ROOT="/tmp/oilrig_chain_${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"
rm -rf "${ROOT}"
rm -f /tmp/esgraph-oilrig-uipc.sock /private/tmp/esgraph-oilrig-uipc.sock 2>/dev/null || true
