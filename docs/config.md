# Configuration

esgraph uses TOML configuration files. All runtime behaviour — ESF subscriptions, LadybugDB database path, batching, path muting — is driven from config.

## Files

| File | Used on | Purpose |
|------|---------|---------|
| [`config/default.toml`](../config/default.toml) | Host dev | Local replay; `data/events.lbug` |
| [`config/vm.default.toml`](../config/vm.default.toml) | VM (`/opt/esgraph`) | Live ESF; absolute DB path |
| [`config/vm.env`](../config/vm.env.example) | Deploy scripts | SSH host, user, install path (gitignored) |

Copy `config/vm.env.example` → `config/vm.env` and set `ESGRAPH_VM_HOST`, `ESGRAPH_VM_USER`, and optionally `ESGRAPH_INSTALL_PATH`.

## `[events]`

Three lists of snake_case event names. See [core — event names](core.md#event-names-eseventname).

### Host default (`default.toml`)

```toml
[events]
process = ["notify_exec", "notify_fork", "notify_exit"]
file = ["notify_create", "notify_write", "notify_unlink", "notify_rename"]
network = ["notify_uipc_bind", "notify_uipc_connect"]
```

`notify_open` is intentionally omitted — very high volume on desktop systems.

### VM default (`vm.default.toml`)

Adds process telemetry useful on the VM:

```toml
process = [
    "notify_exec", "notify_fork", "notify_exit",
    "notify_get_task", "notify_remote_thread_create",
]
```

## `[store]`

```toml
[store]
path = "data/events.lbug"      # host
# path = "/opt/esgraph/data/events.lbug"  # VM
batch_size = 500
flush_interval_ms = 1000
```

| Field | Effect |
|-------|--------|
| `path` | LadybugDB database file path |
| `batch_size` | Max events per ingest transaction in writer / replay |
| `flush_interval_ms` | Max delay before partial batch flush in live `run` |

Under `sudo`, use absolute paths — `~` expands to `/var/root`.

## `[mute]`

```toml
[mute]
paths = ["/System", "/private/var/db"]
```

Prefix paths passed to `es_mute_path` before ESF subscribe. Reduces noise from system activity.

## Validation

At load time:

- Unknown event names → error with list of valid names
- Empty `store.path` → error
- `batch_size == 0` → error

AUTH event names (`auth_exec`, `auth_open`) are accepted in config. The collector responds `ALLOW` to AUTH messages; enabling AUTH increases handler responsibility and volume.

## Programmatic defaults

`Config::default()` in `esgraph-core` mirrors `config/default.toml` for tests and embedded use.
