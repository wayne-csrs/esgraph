# description: Remove Lazarus collection artefacts from the VM
# removes: /tmp/lz_collect_${ESGRAPH_RUN_ID}

COLLECT="/tmp/lz_collect_${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"
rm -rf "${COLLECT}"
