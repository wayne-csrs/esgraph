# description: Remove AMOS stealer chain staging artefacts from the VM
# removes: /tmp/amos_chain_${ESGRAPH_RUN_ID}, /tmp/amos_helper_${ESGRAPH_RUN_ID}, /tmp/amos_helper_ran_${ESGRAPH_RUN_ID}, /tmp/amos_archive_${ESGRAPH_RUN_ID}.tar.gz

CHAIN="/tmp/amos_chain_${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"
rm -rf "${CHAIN}"
rm -f "/tmp/amos_helper_${ESGRAPH_RUN_ID}"
rm -f "/tmp/amos_helper_ran_${ESGRAPH_RUN_ID}"
rm -f "/tmp/amos_archive_${ESGRAPH_RUN_ID}.tar.gz"
