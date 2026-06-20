# VM setup

Live ESF collection needs capabilities and system changes that are a poor fit for your everyday Mac: root, Full Disk Access, code signing with the Endpoint Security entitlement, and often **disabled System Integrity Protection (SIP)** and relaxed AMFI for ad-hoc signed binaries.

**Run esgraph on a dedicated macOS VM**, not your primary workstation. Keep editing and building on the host; deploy and run on the guest. This guide walks through what the VM must satisfy before `esgraphd run` will work.

## Why a dedicated VM

| Concern | On a daily-use Mac | On a dedicated VM |
|---------|-------------------|-------------------|
| SIP disabled | Weakens system-wide protections you rely on | Isolated to a throwaway environment |
| `sudo` + FDA for a collector | Broad access on machine with your data | Scoped to a single-purpose guest |
| Ad-hoc signing with ES entitlement | Awkward alongside normal dev signing | Standard workflow for local ESF work |
| High-volume ESF telemetry | Noise and load on production system | Expected load on an instrumented guest |

SIP off is **not** a substitute for Apple’s other ESF requirements (entitlement, root, FDA). It is an enabler for signing and debugging workflows that are difficult on a locked-down host — which is why isolation matters.

## Host vs VM

| Activity | Host Mac | Dedicated VM |
|----------|----------|----------------|
| Edit source, `cargo build` | Yes | Optional |
| `esgraphd replay` / fixtures | Yes | Yes |
| Live `esgraphd run` | No | **Yes** |
| Ad-hoc codesign, FDA, `sudo` | Avoid | **Yes** |

Host and VM must use the **same CPU architecture** (`arm64` or `x86_64`). Check with `uname -m` on both.

---

## Requirements checklist

Work through these in order. Each section must pass before the next is meaningful.

### 1. VM baseline

Create or pick a macOS guest (11.0 or newer recommended; newer releases expose more ESF event types).

On the **VM**, confirm:

```bash
sw_vers
uname -m
csrutil status
```

| Check | Expected |
|-------|----------|
| macOS version | 11+ (12+ preferred) |
| Architecture | Matches host (`arm64` or `x86_64`) |
| SIP | `disabled` (or disabled for the scenarios you need — required for typical ad-hoc ESF setups) |

If SIP is still enabled and you need it off, reboot to Recovery and run `csrutil disable`, then reboot again. Only do this on the **dedicated VM**.

### 2. SSH access from the host

`deploy-vm.sh` and `debug-vm.sh` run `ssh` and `rsync` non-interactively. **Password prompts will fail** the scripts — set up key-based login first if you do not already have it.

On the **host**, copy and fill in deploy settings:

```bash
cp config/vm.env.example config/vm.env
# Edit: ESGRAPH_VM_HOST, ESGRAPH_VM_USER, ESGRAPH_INSTALL_PATH
```

#### Enable SSH with a key (if needed)

**On the VM** — turn on Remote Login:

- **System Settings → General → Sharing → Remote Login** → On  
- Allow access for your VM user (or “All users” on a dedicated guest).

**On the host** — load your deploy settings (needed for the commands below):

```bash
cd /path/to/esgraph
source config/vm.env
```

**Step A — create a key pair** (skip if you already have `~/.ssh/id_ed25519.pub` or `~/.ssh/id_rsa.pub`):

```bash
ssh-keygen -t ed25519 -f ~/.ssh/id_ed25519 -N ""
```

`-N ""` creates a key with no passphrase so `deploy-vm.sh` can run non-interactively. Use a passphrase only if you also configure `ssh-agent`.

If `ssh-copy-id` later prints `ERROR: No identities found`, you skipped this step or have no `~/.ssh/*.pub` files.

**Step B — install the public key on the VM** (one-time; enter the VM password when prompted):

```bash
ssh-copy-id -i ~/.ssh/id_ed25519.pub "$ESGRAPH_VM_USER@$ESGRAPH_VM_HOST"
```

Explicit `-i` avoids ambiguity when no default key exists.

If `ssh-copy-id` is unavailable:

```bash
cat ~/.ssh/id_ed25519.pub | ssh "$ESGRAPH_VM_USER@$ESGRAPH_VM_HOST" \
  'mkdir -p ~/.ssh && chmod 700 ~/.ssh && cat >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys'
```

**Step C — verify** passwordless login:

```bash
ssh "$ESGRAPH_VM_USER@$ESGRAPH_VM_HOST" 'echo ok'
```

You should see `ok` with no password prompt.

### 2.5 Optional: passwordless `sudo` for automation

If you run `simulate-vm.sh` or other scripted flows, `sudo` password prompts can block non-interactive SSH sessions. On a dedicated VM, you can allow passwordless `sudo` for your VM user.

1. **On the VM**, open a sudoers drop-in with `visudo`:

```bash
sudo visudo -f /etc/sudoers.d/esgraph
```

2. Add this line (replace `esgraph` with your VM username):

```text
esgraph ALL=(root) NOPASSWD: /opt/esgraph/esgraphd, /bin/kill, /bin/cp, /bin/rm, /usr/bin/env
```

3. Save and verify:

```bash
sudo -n /opt/esgraph/esgraphd status --config /opt/esgraph/config/default.toml
sudo -n /bin/kill -l
sudo -n /bin/cp /etc/hosts /tmp/esgraph-sudo-test && rm -f /tmp/esgraph-sudo-test
sudo -n /usr/bin/env true
```

If both commands return without a password prompt, automation is ready.

To revert, remove `/etc/sudoers.d/esgraph` (via `sudo visudo -f /etc/sudoers.d/esgraph`) or remove the added line.

### 3. Endpoint Security entitlement

The binary must include `com.apple.developer.endpoint-security.client` in its signature.

Entitlement plist: [`esgraphd.entitlements`](../esgraphd.entitlements).

On the VM this is applied automatically when you deploy from the host:

```bash
./scripts/deploy-vm.sh
```

That script builds on the host, rsyncs to `/opt/esgraph/`, and ad-hoc signs with the entitlement. Details: [deployment](deployment.md).

**Verify on the VM** after deploy:

```bash
codesign -dv --entitlements - /opt/esgraph/esgraphd 2>&1 | head -20
```

You should see `com.apple.developer.endpoint-security.client`.

### 4. Full Disk Access (FDA)

On the **VM**, open **System Settings → Privacy & Security → Full Disk Access** and add:

- **Terminal** (or iTerm), if you run commands locally on the VM
- The **SSH/shell** environment you use when connecting remotely, if applicable
- **`/opt/esgraph/esgraphd`** after it is deployed and signed

FDA is per-binary and per-app. If you redeploy a new build, re-add the binary if macOS treats it as a new file.

### 5. Root for the ES client

The Endpoint Security client must run as root for the entire time it is subscribed.

On the **VM**:

```bash
sudo /opt/esgraph/esgraphd run --config /opt/esgraph/config/default.toml
```

Do not run live collection as a normal user.

### 6. Install layout

Deploy uses a fixed path so FDA, lldb, and docs stay aligned:

```
/opt/esgraph/
├── esgraphd
├── esgraphd.entitlements
├── config/default.toml    # from config/vm.default.toml
└── data/events.lbug     # created at first run (+ events.lbug.wal)
```

Config on the VM uses an absolute database path (no `~` — under `sudo`, `~` is `/var/root`).

---

## First run

After the checklist above:

```bash
# On host — deploy latest build
./scripts/deploy-vm.sh

# On VM — start collector
# (Not required when working with simulations; the collector will be started automatically as part of the simulation.)
sudo /opt/esgraph/esgraphd run --config /opt/esgraph/config/default.toml
```

Optional verbose logging on the VM:

```bash
sudo RUST_LOG=esgraph=debug /opt/esgraph/esgraphd run --config /opt/esgraph/config/default.toml
```

Stop with **Ctrl+C**. The process flushes pending events to LadybugDB before exit.

**`esgraphd status` is not the collector.** It opens the graph directory, prints node/relationship counts, and exits immediately. If `status` works, that only means the graph exists — not that `esgraphd run` is still active.

Check for a live collector on the VM:

```bash
ps -ax | grep '[/]opt/esgraph/esgraphd run'
```

Stop a stuck collector from the host:

```bash
./scripts/stop-vm-collector.sh
```

Or on the VM:

```bash
sudo pkill -INT -f "/opt/esgraph/esgraphd run"
# if still running:
sudo pkill -KILL -f "/opt/esgraph/esgraphd run"
```

Inspect data on the VM (or copy `events.lbug` and `events.lbug.wal`):

```bash
/opt/esgraph/esgraphd status --config /opt/esgraph/config/default.toml
```

## Three ESF gates (quick reference)

Apple enforces these on every Mac, including your VM. SIP being off does not bypass them.

| Gate | `es_new_client` symptom | What to do |
|------|-------------------------|------------|
| Entitlement | `NOT_ENTITLED` | Deploy with `esgraphd.entitlements`; verify `codesign -d --entitlements -` |
| Root | `NOT_PRIVILEGED` | Prefix command with `sudo` |
| Full Disk Access | `NOT_PERMITTED` | Add binary and shell in System Settings |

Other errors:

| Error | Fix |
|-------|-----|
| `TOO_MANY_CLIENTS` | Another ES client is connected; stop it first |

---

## Debugging

Interactive lldb on the VM over SSH from the host:

```bash
./scripts/debug-vm.sh
```

Inside lldb:

```
(lldb) run -- --config /opt/esgraph/config/default.toml
(lldb) process attach --name esgraphd
(lldb) bt
```

Use a **debug** deploy (default `./scripts/deploy-vm.sh`, not `--release`) so symbols resolve. See [deployment](deployment.md).

---

## Summary

1. Use a **dedicated VM** — SIP/AMFI/signing/root/FDA are intentional there, not on your host.
2. Complete the **checklist**: baseline → SSH → deploy/sign → FDA → `sudo run`.
3. Use **`/opt/esgraph`** as the install root and [deployment](deployment.md) scripts for repeatability.
