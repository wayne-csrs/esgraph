# ESF collector (`esgraph-esf`)

`esgraph-esf` subscribes to macOS Endpoint Security Framework events, normalises each message into [`NormalisedEvent`](core.md#normalisedevent), and sends them to the LadybugDB writer via a channel. The full implementation is **macOS only** (`endpoint-sec` 0.6 with `macos_11_0_0` feature). On other platforms, `run_collector` returns `EsfError::UnsupportedPlatform`.

## Architecture

```
┌─────────────────────────────────────┐
│  Main thread: run_collector         │
│  - Client::new(handler)             │
│  - subscribe(es_event_type_t[])     │
│  - handler → normalise → mpsc send│
└──────────────┬──────────────────────┘
               │ sync_channel (bounded)
               ▼
┌─────────────────────────────────────┐
│  Writer thread (esgraphd)           │
│  - batch + flush → GraphStore       │
└─────────────────────────────────────┘
```

`endpoint_sec::Client` is neither `Send` nor `Sync`. The client must be created, used, and dropped on the same thread that calls `run_collector`.

## `run_collector`

```rust
pub fn run_collector(
    config: &Config,
    tx: SyncSender<NormalisedEvent>,
    shutdown: Arc<AtomicBool>,
) -> Result<(), EsfError>
```

Steps:

1. **`init_runtime_version`** — reads `sw_vers -productVersion` and calls `endpoint_sec::version::set_runtime_version`. The `endpoint-sec` crate defaults to 10.15.0; setting the real OS version unlocks newer APIs (e.g. `notify_remote_thread_create`).
2. **`resolved_event_names`** — loads configured `EsEventName` list from TOML.
3. **`event_names_to_es_types`** — maps to `es_event_type_t` constants.
4. **`Client::new`** — registers the message handler; failures produce `EsfError::Client` with operator hints (see below).
5. **`apply_path_mutes`** — for each `[mute].paths` entry, calls `es_mute_path` with `ES_MUTE_PATH_TYPE_PREFIX`.
6. **`subscribe`** — registers for all configured event types.
7. **Shutdown loop** — polls `shutdown` every 200 ms until `AtomicBool` is set (Ctrl+C in `esgraphd`).

## Message handling

For each `Message`:

1. **AUTH response** — if `msg.action()` is `Action::Auth`, respond `ES_AUTH_RESULT_ALLOW` via `respond_auth_result`. Unanswered AUTH events can cause macOS to terminate the client.
2. **Normalise** — `normalise_message(msg)` → `Option<NormalisedEvent>`.
3. **Enqueue** — send on `tx`; if the writer disconnected, log and drop.

Unmodeled ESF variants return `Ok(None)` and are skipped. Normalise errors are logged and skipped.

## Subscription mapping (`subscribe.rs`)

| `EsEventName` | `es_event_type_t` |
|---------------|-------------------|
| `notify_exec` | `ES_EVENT_TYPE_NOTIFY_EXEC` |
| `notify_fork` | `ES_EVENT_TYPE_NOTIFY_FORK` |
| `notify_exit` | `ES_EVENT_TYPE_NOTIFY_EXIT` |
| `notify_remote_thread_create` | `ES_EVENT_TYPE_NOTIFY_REMOTE_THREAD_CREATE` |
| `notify_get_task` | `ES_EVENT_TYPE_NOTIFY_GET_TASK` |
| `auth_exec` | `ES_EVENT_TYPE_AUTH_EXEC` |
| `notify_create` | `ES_EVENT_TYPE_NOTIFY_CREATE` |
| `notify_write` | `ES_EVENT_TYPE_NOTIFY_WRITE` |
| `notify_unlink` | `ES_EVENT_TYPE_NOTIFY_UNLINK` |
| `notify_rename` | `ES_EVENT_TYPE_NOTIFY_RENAME` |
| `notify_open` | `ES_EVENT_TYPE_NOTIFY_OPEN` |
| `notify_close` | `ES_EVENT_TYPE_NOTIFY_CLOSE` |
| `auth_open` | `ES_EVENT_TYPE_AUTH_OPEN` |
| `notify_uipc_bind` | `ES_EVENT_TYPE_NOTIFY_UIPC_BIND` |
| `notify_uipc_connect` | `ES_EVENT_TYPE_NOTIFY_UIPC_CONNECT` |

## Normalisation (`normalise.rs`)

| ESF event | Nodes | Edge |
|-----------|-------|------|
| `notify_exec` / `auth_exec` | parent + child process | `EXECUTED` (parent → child) |
| `notify_fork` | parent + child | `FORKED` |
| `notify_exit` | process | `EXITED` (self-loop) |
| `notify_create` / `auth_create` | process + file | `CREATED` |
| `notify_write` | process + file | `WROTE` |
| `notify_unlink` / `auth_unlink` | process + file | `DELETED` |
| `notify_rename` / `auth_rename` | process + source file | `RENAMED` (+ destination in metadata) |
| `notify_open` / `auth_open` | process + file | `OPENED` |
| `notify_close` | process + file | `CLOSED` |
| `notify_uipc_bind` | process + socket | `BOUND` |
| `notify_uipc_connect` | process + socket | `CONNECTED` |
| `notify_get_task` / `auth_get_task` | process | `GOT_TASK` (self-loop) |
| `notify_remote_thread_create` | process | `INJECTED_THREAD` (self-loop) |

### Field accessors

- **Create** — `EventCreate::destination()` (existing file or new path under directory)
- **Open** — `EventOpen::file()` (not `target()`)
- **Write / unlink / close** — `target()` on the event struct
- **Rename** — `source()` + `destination()` (`ExistingFile` or `NewPath`)
- **UIPC bind** — `dir()` + `filename()` joined as socket path
- **UIPC connect** — `file()` (socket path)

### Process fields

From `Message::process()` / event targets via `process_node()`:

- `audit_token` → `audit_token_hex` (`format!("{token:x}")`)
- `executable().path()`, `signing_id()`, `team_id()`, `cdhash()` (20-byte hex), `ppid()`

## Path muting

Configured prefixes (default: `/System`, `/private/var/db`) reduce volume from system paths before events reach the handler. Uses `Client::mute_path` with prefix type.

## Client creation errors

`format_client_error` augments common `NewClientError` messages:

| Pattern | Hint |
|---------|------|
| `NOT_ENTITLED` | Embed entitlement and sign (see [VM setup](vm-setup.md)) |
| `NOT_PERMITTED` | Grant Full Disk Access |
| `NOT_PRIVILEGED` | Run with `sudo` |

## Errors (`EsfError`)

| Variant | Cause |
|---------|-------|
| `UnsupportedPlatform` | Not macOS |
| `Subscription` | Config / name mapping failure |
| `Client` | `es_new_client`, subscribe, mute failures |
| `Normalise` | Missing required fields in message |

## Entitlements

Live collection requires `com.apple.developer.endpoint-security.client` in the code signature. See [`esgraphd.entitlements`](../esgraphd.entitlements) and [deployment](deployment.md).
