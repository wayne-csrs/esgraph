//! Map live `endpoint_sec::Message` values to [`NormalisedEvent`].

use std::ffi::OsStr;

use endpoint_sec::{
    Event, EventCreateDestinationFile, EventRenameDestinationFile, File, Message, Process,
};
use esgraph_core::{
    EdgeKind, EventDetails, FileNode, GraphEdge, GraphNode, NormalisedEvent, ProcessIdentity,
    ProcessNode,
};

use crate::error::EsfError;
use crate::subscribe::es_type_to_event_name;

/// Convert one ESF message. Returns `None` for event types we do not model yet.
pub fn normalise_message(msg: &Message) -> Result<Option<NormalisedEvent>, EsfError> {
    let Some(event) = msg.event() else {
        return Ok(None);
    };
    let event_name = es_type_to_event_name(msg.event_type())
        .ok_or_else(|| EsfError::Normalise(format!("unsupported event type: {:?}", msg.event_type())))?
        .to_string();
    let timestamp_unix_ns = msg_time_ns(msg);
    let instigator = msg.process();

    let (nodes, edges, details) = match event {
        Event::NotifyExec(ev) => {
            let args: Vec<String> = ev
                .args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            let target = ev.target();
            let target_path = process_path(&target);
            let child = process_node(&target, Some(&args), None);
            let parent = process_node(&instigator, None, None);
            let mut details = instigator_details(&instigator).with_exec(&target_path, &args);
            details.target_audit_token_hex = Some(audit_token_hex(&target));
            (
                vec![GraphNode::Process(parent), GraphNode::Process(child)],
                vec![process_process_edge(
                    &instigator,
                    &target,
                    EdgeKind::Executed,
                    timestamp_unix_ns,
                )],
                details,
            )
        }
        Event::NotifyFork(ev) => {
            let child = process_node(&ev.child(), None, None);
            let parent = process_node(&instigator, None, None);
            (
                vec![GraphNode::Process(parent), GraphNode::Process(child)],
                vec![process_process_edge(
                    &instigator,
                    &ev.child(),
                    EdgeKind::Forked,
                    timestamp_unix_ns,
                )],
                instigator_details(&instigator),
            )
        }
        Event::NotifyExit(ev) => {
            let status = ev.stat();
            let proc = process_node(&instigator, None, Some(status));
            let details = instigator_details(&instigator).with_exit(status);
            (
                vec![GraphNode::Process(proc.clone())],
                vec![GraphEdge {
                    kind: EdgeKind::Exited,
                    src_id: proc.identity.audit_token_hex.clone(),
                    src_kind: "process".into(),
                    dst_id: proc.identity.audit_token_hex,
                    dst_kind: "process".into(),
                    timestamp_unix_ns,
                    metadata: Some(format!(r#"{{"exit_status":{status}}}"#)),
                }],
                details,
            )
        }
        Event::NotifyRemoteThreadCreate(ev) => {
            let target = ev.target();
            (
                vec![
                    GraphNode::Process(process_node(&instigator, None, None)),
                    GraphNode::Process(process_node(&target, None, None)),
                ],
                vec![process_process_edge(
                    &instigator,
                    &target,
                    EdgeKind::RemoteThreadCreated,
                    timestamp_unix_ns,
                )],
                instigator_details(&instigator),
            )
        }
        Event::NotifyGetTask(ev) => {
            let target = ev.target();
            (
                vec![
                    GraphNode::Process(process_node(&instigator, None, None)),
                    GraphNode::Process(process_node(&target, None, None)),
                ],
                vec![process_process_edge(
                    &instigator,
                    &target,
                    EdgeKind::GotTask,
                    timestamp_unix_ns,
                )],
                instigator_details(&instigator),
            )
        }
        Event::NotifyCreate(ev) => match ev.destination() {
            Some(EventCreateDestinationFile::ExistingFile { file, .. }) => file_event(
                &instigator,
                &file,
                EdgeKind::Created,
                timestamp_unix_ns,
            ),
            Some(EventCreateDestinationFile::NewPath {
                directory,
                filename,
                mode,
                ..
            }) => path_file_event(
                &instigator,
                &join_dir_filename(&directory, filename),
                Some(mode as u32),
                EdgeKind::Created,
                timestamp_unix_ns,
            ),
            None => return Ok(None),
            Some(_) => return Ok(None),
        },
        Event::NotifyWrite(ev) => file_event(
            &instigator,
            &ev.target(),
            EdgeKind::Wrote,
            timestamp_unix_ns,
        ),
        Event::NotifyUnlink(ev) => file_event(
            &instigator,
            &ev.target(),
            EdgeKind::Unlinked,
            timestamp_unix_ns,
        ),
        Event::NotifyRename(ev) => {
            let (file, file_details) = file_from_esf(&ev.source());
            let proc = process_node(&instigator, None, None);
            let dest = rename_dest_path(&ev);
            let details = instigator_details(&instigator).merge_file(&file_details);
            (
                vec![GraphNode::Process(proc), GraphNode::File(file)],
                vec![GraphEdge {
                    kind: EdgeKind::Renamed,
                    src_id: audit_token_hex(&instigator),
                    src_kind: "process".into(),
                    dst_id: file_path(&ev.source()),
                    dst_kind: "file".into(),
                    timestamp_unix_ns,
                    metadata: Some(format!(r#"{{"destination":"{dest}"}}"#)),
                }],
                details,
            )
        }
        Event::NotifyOpen(ev) => file_event(
            &instigator,
            &ev.file(),
            EdgeKind::Opened,
            timestamp_unix_ns,
        ),
        Event::NotifyClose(ev) => file_event(
            &instigator,
            &ev.target(),
            EdgeKind::Closed,
            timestamp_unix_ns,
        ),
        Event::NotifyUipcBind(ev) => socket_path_event(
            &instigator,
            &join_dir_filename(&ev.dir(), ev.filename()),
            Some(ev.mode() as u32),
            EdgeKind::UipcBound,
            timestamp_unix_ns,
        ),
        Event::NotifyUipcConnect(ev) => socket_event(
            &instigator,
            &ev.file(),
            EdgeKind::UipcConnected,
            timestamp_unix_ns,
        ),
        _ => return Ok(None),
    };

    let details = if details.is_empty() {
        None
    } else {
        Some(details)
    };

    Ok(Some(NormalisedEvent {
        event_name,
        timestamp_unix_ns,
        nodes,
        edges,
        details,
        raw_json: None,
    }))
}

fn msg_time_ns(msg: &Message) -> i64 {
    msg.time()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            i64::try_from(d.as_secs())
                .unwrap_or(0)
                .saturating_mul(1_000_000_000)
                .saturating_add(i64::try_from(d.subsec_nanos()).unwrap_or(0))
        })
        .unwrap_or(0)
}

fn audit_token_hex(p: &Process<'_>) -> String {
    format!("{:x}", p.audit_token())
}

fn process_path(p: &Process<'_>) -> String {
    p.executable().path().to_string_lossy().into_owned()
}

fn file_path(f: &File<'_>) -> String {
    f.path().to_string_lossy().into_owned()
}

fn cdhash_hex(p: &Process<'_>) -> Option<String> {
    let h = p.cdhash();
    if h.iter().all(|&b| b == 0) {
        None
    } else {
        Some(
            h.iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        )
    }
}

fn args_json(args: &[String]) -> String {
    serde_json::to_string(args).unwrap_or_else(|_| "[]".into())
}

fn process_node(p: &Process<'_>, exec_args: Option<&[String]>, exit_status: Option<i32>) -> ProcessNode {
    let token = p.audit_token();
    let signing = p.signing_id();
    let team = p.team_id();
    ProcessNode {
        identity: ProcessIdentity {
            audit_token_hex: audit_token_hex(p),
            pid: p.audit_token().pid(),
        },
        path: process_path(p),
        signing_id: if signing.is_empty() {
            None
        } else {
            Some(signing.to_string_lossy().into_owned())
        },
        team_id: if team.is_empty() {
            None
        } else {
            Some(team.to_string_lossy().into_owned())
        },
        cdhash: cdhash_hex(p),
        ppid: Some(p.ppid()),
        euid: Some(token.euid()),
        egid: Some(token.egid()),
        ruid: Some(token.ruid()),
        rgid: Some(token.rgid()),
        session_id: Some(p.session_id()),
        is_platform_binary: Some(p.is_platform_binary()),
        parent_audit_token_hex: p
            .parent_audit_token()
            .map(|t| format!("{:x}", t)),
        args_json: exec_args.map(|a| args_json(a)),
        exit_status,
    }
}

fn file_from_esf(f: &File<'_>) -> (FileNode, EventDetails) {
    let st = f.stat();
    let file = FileNode {
        path: file_path(f),
        inode: Some(st.st_ino as u64),
        mode: Some(st.st_mode as u32),
        owner_uid: Some(st.st_uid),
        owner_gid: Some(st.st_gid),
        path_truncated: f.path_truncated(),
    };
    let details = EventDetails {
        file_mode: file.mode,
        file_inode: file.inode,
        file_owner_uid: file.owner_uid,
        file_owner_gid: file.owner_gid,
        path_truncated: Some(file.path_truncated),
        ..Default::default()
    };
    (file, details)
}

fn instigator_details(p: &Process<'_>) -> EventDetails {
    let token = p.audit_token();
    EventDetails {
        instigator_pid: Some(token.pid()),
        instigator_euid: Some(token.euid()),
        instigator_egid: Some(token.egid()),
        instigator_path: Some(process_path(p)),
        instigator_audit_token_hex: Some(audit_token_hex(p)),
        ..Default::default()
    }
}

trait EventDetailsExt {
    fn with_exec(self, target_path: &str, args: &[String]) -> EventDetails;
    fn with_exit(self, status: i32) -> EventDetails;
    fn merge_file(self, file: &EventDetails) -> EventDetails;
}

impl EventDetailsExt for EventDetails {
    fn with_exec(mut self, target_path: &str, args: &[String]) -> EventDetails {
        self.target_path = Some(target_path.to_string());
        self.exec_args = Some(args.to_vec());
        self
    }

    fn with_exit(mut self, status: i32) -> EventDetails {
        self.exit_status = Some(status);
        self
    }

    fn merge_file(mut self, file: &EventDetails) -> EventDetails {
        self.file_mode = file.file_mode;
        self.file_inode = file.file_inode;
        self.file_owner_uid = file.file_owner_uid;
        self.file_owner_gid = file.file_owner_gid;
        self.path_truncated = file.path_truncated;
        self
    }
}

fn join_dir_filename(dir: &File<'_>, name: &OsStr) -> String {
    let mut path = dir.path().to_os_string();
    path.push("/");
    path.push(name);
    path.to_string_lossy().into_owned()
}

fn rename_dest_path(ev: &endpoint_sec::EventRename<'_>) -> String {
    match ev.destination() {
        Some(EventRenameDestinationFile::ExistingFile { file, .. }) => file_path(&file),
        Some(EventRenameDestinationFile::NewPath {
            directory,
            filename,
            ..
        }) => {
            join_dir_filename(&directory, filename)
        }
        None => String::new(),
        Some(_) => String::new(),
    }
}

fn path_file_event(
    proc: &Process<'_>,
    path: &str,
    mode: Option<u32>,
    kind: EdgeKind,
    ts: i64,
) -> (Vec<GraphNode>, Vec<GraphEdge>, EventDetails) {
    let file = FileNode {
        path: path.to_string(),
        inode: None,
        mode,
        owner_uid: None,
        owner_gid: None,
        path_truncated: false,
    };
    let file_details = EventDetails {
        file_mode: mode,
        ..Default::default()
    };
    let details = instigator_details(proc).merge_file(&file_details);
    (
        vec![
            GraphNode::Process(process_node(proc, None, None)),
            GraphNode::File(file),
        ],
        vec![GraphEdge {
            kind,
            src_id: audit_token_hex(proc),
            src_kind: "process".into(),
            dst_id: path.to_string(),
            dst_kind: "file".into(),
            timestamp_unix_ns: ts,
            metadata: None,
        }],
        details,
    )
}

fn socket_path_event(
    proc: &Process<'_>,
    path: &str,
    socket_mode: Option<u32>,
    kind: EdgeKind,
    ts: i64,
) -> (Vec<GraphNode>, Vec<GraphEdge>, EventDetails) {
    let mut details = instigator_details(proc);
    details.file_mode = socket_mode;
    (
        vec![
            GraphNode::Process(process_node(proc, None, None)),
            GraphNode::Socket(esgraph_core::SocketNode {
                path: path.to_string(),
            }),
        ],
        vec![GraphEdge {
            kind,
            src_id: audit_token_hex(proc),
            src_kind: "process".into(),
            dst_id: path.to_string(),
            dst_kind: "socket".into(),
            timestamp_unix_ns: ts,
            metadata: None,
        }],
        details,
    )
}

fn file_event(
    proc: &Process<'_>,
    file: &File<'_>,
    kind: EdgeKind,
    ts: i64,
) -> (Vec<GraphNode>, Vec<GraphEdge>, EventDetails) {
    let (file_node, file_details) = file_from_esf(file);
    let details = instigator_details(proc).merge_file(&file_details);
    (
        vec![
            GraphNode::Process(process_node(proc, None, None)),
            GraphNode::File(file_node),
        ],
        vec![file_process_edge(proc, file, kind, ts)],
        details,
    )
}

fn socket_event(
    proc: &Process<'_>,
    file: &File<'_>,
    kind: EdgeKind,
    ts: i64,
) -> (Vec<GraphNode>, Vec<GraphEdge>, EventDetails) {
    let path = file_path(file);
    (
        vec![
            GraphNode::Process(process_node(proc, None, None)),
            GraphNode::Socket(esgraph_core::SocketNode { path: path.clone() }),
        ],
        vec![GraphEdge {
            kind,
            src_id: audit_token_hex(proc),
            src_kind: "process".into(),
            dst_id: path,
            dst_kind: "socket".into(),
            timestamp_unix_ns: ts,
            metadata: None,
        }],
        instigator_details(proc),
    )
}

fn file_process_edge(proc: &Process<'_>, file: &File<'_>, kind: EdgeKind, ts: i64) -> GraphEdge {
    GraphEdge {
        kind,
        src_id: audit_token_hex(proc),
        src_kind: "process".into(),
        dst_id: file_path(file),
        dst_kind: "file".into(),
        timestamp_unix_ns: ts,
        metadata: None,
    }
}

fn process_process_edge(
    src: &Process<'_>,
    dst: &Process<'_>,
    kind: EdgeKind,
    ts: i64,
) -> GraphEdge {
    GraphEdge {
        kind,
        src_id: audit_token_hex(src),
        src_kind: "process".into(),
        dst_id: audit_token_hex(dst),
        dst_kind: "process".into(),
        timestamp_unix_ns: ts,
        metadata: None,
    }
}
