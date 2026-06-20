# description: Remove APT29 discovery staging artefacts from the VM
# removes: /tmp/apt29_chain_${ESGRAPH_RUN_ID}

CHAIN_ROOT="/tmp/apt29_chain_${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"
rm -rf "${CHAIN_ROOT}"
