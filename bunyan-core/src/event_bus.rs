//! In-process event bus.
//!
//! Bunyan fires lifecycle events as part of its routes (workspace.created,
//! claude.stopped, etc.). Today those events drive the on-disk hook
//! executor. v6 makes the bus a first-class concept: a broadcast channel
//! that the hook executor reads from AND external HTTP subscribers can tail
//! over SSE.
//!
//! The bus is opt-in. If no one ever creates an `EventBus`, the hook code
//! path stays identical to v5. If routes do create one and publish to it,
//! subscribers see events; nothing else changes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// What gets broadcast on the bus. Mirrors the hook stdin payload so
/// external clients can demux exactly the way on-disk hooks do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event: String,
    pub version: u32,
    pub timestamp: String,
    pub repo: Option<RepoRef>,
    pub workspace: Option<WorkspaceRef>,
    pub extras: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRef {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRef {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub branch: Option<String>,
}

/// The in-process broadcast bus. Cheap to clone; producers fan-out to all
/// active subscribers.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<EventEnvelope>,
}

impl EventBus {
    /// Buffer up to `capacity` recent events for slow subscribers. New
    /// subscribers connecting after a burst may miss the oldest events —
    /// that's the standard broadcast-channel trade-off.
    pub fn new(capacity: usize) -> Arc<Self> {
        let (tx, _) = broadcast::channel(capacity);
        Arc::new(Self { tx })
    }

    /// Publish an event. Returns the number of currently-active subscribers
    /// that received it (mostly useful for tests / debugging).
    pub fn publish(&self, env: EventEnvelope) -> usize {
        self.tx.send(env).unwrap_or(0)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self {
            tx: broadcast::channel(256).0,
        }
    }
}

/// Convert a `HookContext` into a publishable envelope. Used by the
/// hook-firing path so SSE subscribers see the same shape on-disk hooks do.
pub fn envelope_from_context(ctx: &crate::hooks::HookContext) -> EventEnvelope {
    let payload = ctx.build_payload();
    EventEnvelope {
        event: ctx.event.clone(),
        version: crate::hooks::PAYLOAD_VERSION,
        timestamp: payload
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        repo: ctx.repo_id.as_ref().map(|id| RepoRef {
            id: id.clone(),
            name: ctx.repo_name.clone().unwrap_or_default(),
            path: ctx.repo_root_path.clone(),
        }),
        workspace: ctx.workspace_id.as_ref().map(|id| WorkspaceRef {
            id: id.clone(),
            name: ctx.workspace_name.clone().unwrap_or_default(),
            path: ctx.workspace_path.clone(),
            branch: ctx.branch.clone(),
        }),
        extras: payload
            .get("extras")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::HookContext;

    #[tokio::test]
    async fn subscriber_receives_published_envelope() {
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        let env = EventEnvelope {
            event: "workspace.created".into(),
            version: 1,
            timestamp: "t".into(),
            repo: None,
            workspace: None,
            extras: serde_json::Value::Null,
        };
        let n = bus.publish(env.clone());
        assert_eq!(n, 1);
        let received = rx.recv().await.unwrap();
        assert_eq!(received.event, "workspace.created");
    }

    #[tokio::test]
    async fn publish_with_no_subscribers_returns_zero() {
        let bus = EventBus::new(8);
        let n = bus.publish(EventEnvelope {
            event: "x".into(),
            version: 1,
            timestamp: "t".into(),
            repo: None,
            workspace: None,
            extras: serde_json::Value::Null,
        });
        assert_eq!(n, 0);
    }

    #[test]
    fn envelope_from_context_copies_repo_workspace_and_extras() {
        let ctx = HookContext::new("workspace.created")
            .with_repo("frontend", "r-id")
            .with_workspace("ws", "w-id", "/tmp/x")
            .with_branch("fix")
            .with_repo_root("/tmp/frontend")
            .with_extra("key", "val");
        let env = envelope_from_context(&ctx);
        assert_eq!(env.event, "workspace.created");
        assert_eq!(env.repo.as_ref().unwrap().id, "r-id");
        assert_eq!(env.repo.as_ref().unwrap().name, "frontend");
        assert_eq!(env.workspace.as_ref().unwrap().id, "w-id");
        assert_eq!(env.workspace.as_ref().unwrap().branch.as_deref(), Some("fix"));
        assert_eq!(env.extras["key"], "val");
    }

    #[tokio::test]
    async fn slow_subscriber_lags_but_doesnt_break_others() {
        // Capacity 2; publish 5 events. The slow subscriber should see Lagged.
        let bus = EventBus::new(2);
        let mut rx = bus.subscribe();
        for i in 0..5 {
            bus.publish(EventEnvelope {
                event: format!("e{}", i),
                version: 1,
                timestamp: "t".into(),
                repo: None,
                workspace: None,
                extras: serde_json::Value::Null,
            });
        }
        // First recv() should return a Lagged error containing the number of
        // skipped messages.
        match rx.recv().await {
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => { /* expected */ }
            Ok(env) => panic!("expected Lagged, got {:?}", env),
            Err(e) => panic!("expected Lagged, got {:?}", e),
        }
    }
}
