//! RUST-02A: only in-memory session operations cross the webview boundary.
use hmi_engineering_core::{
    dispatch, ProjectSession, ProjectSnapshot, Request, Response, SessionConfig, Transaction,
};
use hmi_project_model::{load_project, MigrationRegistry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "request", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkspaceRequest {
    Snapshot {},
    Validate {},
    Execute {
        transaction: Transaction,
    },
    #[serde(rename_all = "camelCase")]
    Undo {
        expected_revision: u64,
    },
    #[serde(rename_all = "camelCase")]
    Redo {
        expected_revision: u64,
    },
}

impl WorkspaceRequest {
    pub fn check_context(
        &self,
        generation: Option<u64>,
        current_generation: u64,
        pending_recovery: bool,
    ) -> Result<(), String> {
        if matches!(self, Self::Snapshot {} | Self::Validate {}) {
            return Ok(());
        }
        if pending_recovery {
            return Err("Resolve the untitled recovery before editing".into());
        }
        if generation != Some(current_generation) {
            return Err("Project session changed. Refresh before editing".into());
        }
        if matches!(self, Self::Execute { transaction } if transaction.expected_revision.is_none())
        {
            return Err("Project revision required for editing".into());
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct WorkspaceReply {
    response: Response,
    snapshot: ProjectSnapshot,
}

impl WorkspaceReply {
    pub fn revision(&self) -> u64 {
        self.snapshot.revision
    }
}

pub fn initial_session() -> ProjectSession {
    let project = load_project(include_str!(
        "../../crates/project-model/fixtures/valid/minimal.hmi.json"
    ))
    .expect("embedded project must validate")
    .project;
    ProjectSession::new(project, SessionConfig::default())
}

pub fn request(session: &mut ProjectSession, request: WorkspaceRequest) -> WorkspaceReply {
    let request = protocol_request(request);
    let (response, _) = dispatch(session, request, &MigrationRegistry::builtin());
    WorkspaceReply {
        response,
        snapshot: session.snapshot(),
    }
}

fn protocol_request(request: WorkspaceRequest) -> Request {
    match request {
        WorkspaceRequest::Snapshot {} => Request::Snapshot {},
        WorkspaceRequest::Validate {} => Request::Validate {},
        WorkspaceRequest::Execute { transaction } => Request::Execute { transaction },
        WorkspaceRequest::Undo { expected_revision } => Request::Undo {
            expected_revision: Some(expected_revision),
        },
        WorkspaceRequest::Redo { expected_revision } => Request::Redo {
            expected_revision: Some(expected_revision),
        },
    }
}

pub fn review_request(
    review: &mut hmi_engineering_core::legacy_quarantine::QuarantinedImport,
    request: WorkspaceRequest,
) -> WorkspaceReply {
    let response = review.request(protocol_request(request));
    WorkspaceReply {
        response,
        snapshot: review.snapshot(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmi_engineering_core::Command;
    #[test]
    fn edits_undo_redo_and_rejects_stale_revision() {
        let mut s = initial_session();
        let id = s.project().screens[0].id.clone();
        let transaction = Transaction::single(Command::RenameScreen {
            screen: id,
            name: "Overview".into(),
        })
        .at_revision(0);
        let reply = request(
            &mut s,
            WorkspaceRequest::Execute {
                transaction: transaction.clone(),
            },
        );
        assert!(matches!(reply.response, Response::Changed { .. }));
        assert_eq!(reply.snapshot.project.screens[0].name, "Overview");
        let stale = request(&mut s, WorkspaceRequest::Execute { transaction });
        assert!(matches!(stale.response, Response::Error { .. }));
        assert_eq!(stale.snapshot.revision, 1);
        assert!(matches!(
            request(
                &mut s,
                WorkspaceRequest::Undo {
                    expected_revision: 1
                }
            )
            .response,
            Response::Changed { .. }
        ));
        assert!(!s.is_dirty());
        assert!(matches!(
            request(
                &mut s,
                WorkspaceRequest::Redo {
                    expected_revision: 2
                }
            )
            .response,
            Response::Changed { .. }
        ));
        assert!(s.is_dirty());
    }
    #[test]
    fn webview_cannot_request_persistence_or_paths() {
        for input in [
            r#"{"request":"saveAs","path":"/tmp/project.json"}"#,
            r#"{"request":"save"}"#,
            r#"{"request":"autosave"}"#,
            r#"{"request":"reload","force":true}"#,
            r#"{"request":"close","force":true}"#,
            r#"{"request":"snapshot","path":"/tmp/project.json"}"#,
        ] {
            assert!(
                serde_json::from_str::<WorkspaceRequest>(input).is_err(),
                "{input}"
            );
        }
    }

    #[test]
    fn component_insert_uses_authoritative_allocator() {
        let mut s = initial_session();
        let request = serde_json::from_str::<WorkspaceRequest>(r#"{"request":"execute","transaction":{"commands":[{"op":"insertNode","parent":"n-root","node":{"id":"pending","element":{"type":"component","kind":"gauge"},"properties":{"value":0,"min":0,"max":100}},"assignNewIds":true}],"expectedRevision":0}}"#).unwrap();
        let reply = super::request(&mut s, request);
        assert!(matches!(reply.response, Response::Changed { .. }));
        let children = &reply.snapshot.project.screens[0].root.children;
        assert_eq!(children.len(), 2);
        assert!(reply.snapshot.status.dirty);
        let Response::Changed { outcome } = reply.response else {
            panic!()
        };
        assert_eq!(
            outcome.allocated_ids["0:node:pending"],
            children[1].id.as_str()
        );
    }
}
