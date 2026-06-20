# description: Remove Wizard Spider staging artefacts from the VM
# removes: /tmp/ws_stage_${ESGRAPH_RUN_ID}

STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"
rm -rf "${STAGE}"
