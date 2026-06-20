//! Map [`EsEventName`] to Apple `es_event_type_t` constants for `es_subscribe`.

use esgraph_core::EsEventName;

use crate::error::EsfError;

/// Build the `es_event_type_t` slice for `Client::subscribe`.
pub fn event_names_to_es_types(
    names: &[EsEventName],
) -> Result<Vec<endpoint_sec::sys::es_event_type_t>, EsfError> {
    names.iter().copied().map(es_event_name_to_type).collect()
}

fn es_event_name_to_type(name: EsEventName) -> Result<endpoint_sec::sys::es_event_type_t, EsfError> {
    use endpoint_sec::sys::es_event_type_t;
    let ty = match name {
        EsEventName::NotifyExec => es_event_type_t::ES_EVENT_TYPE_NOTIFY_EXEC,
        EsEventName::NotifyFork => es_event_type_t::ES_EVENT_TYPE_NOTIFY_FORK,
        EsEventName::NotifyExit => es_event_type_t::ES_EVENT_TYPE_NOTIFY_EXIT,
        EsEventName::NotifyRemoteThreadCreate => es_event_type_t::ES_EVENT_TYPE_NOTIFY_REMOTE_THREAD_CREATE,
        EsEventName::NotifyGetTask => es_event_type_t::ES_EVENT_TYPE_NOTIFY_GET_TASK,
        EsEventName::AuthExec => es_event_type_t::ES_EVENT_TYPE_AUTH_EXEC,
        EsEventName::NotifyCreate => es_event_type_t::ES_EVENT_TYPE_NOTIFY_CREATE,
        EsEventName::NotifyWrite => es_event_type_t::ES_EVENT_TYPE_NOTIFY_WRITE,
        EsEventName::NotifyUnlink => es_event_type_t::ES_EVENT_TYPE_NOTIFY_UNLINK,
        EsEventName::NotifyRename => es_event_type_t::ES_EVENT_TYPE_NOTIFY_RENAME,
        EsEventName::NotifyOpen => es_event_type_t::ES_EVENT_TYPE_NOTIFY_OPEN,
        EsEventName::NotifyClose => es_event_type_t::ES_EVENT_TYPE_NOTIFY_CLOSE,
        EsEventName::AuthOpen => es_event_type_t::ES_EVENT_TYPE_AUTH_OPEN,
        EsEventName::NotifyUipcBind => es_event_type_t::ES_EVENT_TYPE_NOTIFY_UIPC_BIND,
        EsEventName::NotifyUipcConnect => es_event_type_t::ES_EVENT_TYPE_NOTIFY_UIPC_CONNECT,
    };
    Ok(ty)
}

/// Map an ESF event type constant back to our config/event name string.
pub fn es_type_to_event_name(ty: endpoint_sec::sys::es_event_type_t) -> Option<&'static str> {
    use endpoint_sec::sys::es_event_type_t;
    let name = match ty {
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_EXEC => "notify_exec",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_FORK => "notify_fork",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_EXIT => "notify_exit",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_REMOTE_THREAD_CREATE => "notify_remote_thread_create",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_GET_TASK => "notify_get_task",
        es_event_type_t::ES_EVENT_TYPE_AUTH_EXEC => "auth_exec",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_CREATE => "notify_create",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_WRITE => "notify_write",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_UNLINK => "notify_unlink",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_RENAME => "notify_rename",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_OPEN => "notify_open",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_CLOSE => "notify_close",
        es_event_type_t::ES_EVENT_TYPE_AUTH_OPEN => "auth_open",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_UIPC_BIND => "notify_uipc_bind",
        es_event_type_t::ES_EVENT_TYPE_NOTIFY_UIPC_CONNECT => "notify_uipc_connect",
        _ => return None,
    };
    Some(name)
}
