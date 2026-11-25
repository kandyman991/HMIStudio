//! A small in-memory OPC UA server used by the Rust integration tests and, through the
//! `hmi-opcua-test-server` binary, by the Kotlin end-to-end tests. It is not a simulator of any
//! machine; it only gives the driver something real to talk OPC UA to.
//!
//! Namespace `urn:hmistudio:test`, string node ids:
//! `Demo.Running` Boolean rw · `Demo.Counter` Int32 ro (increments) · `Demo.Speed` Float rw ·
//! `Demo.Setpoint` Int16 rw · `Demo.Name` String rw · `Demo.Locked` Double ro
//!
//! Endpoints: `None` and `Basic256Sha256`/`SignAndEncrypt`; anonymous or user
//! `operator` / `operator-password`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opcua::crypto::SecurityPolicy;
use opcua::server::address_space::Variable;
use opcua::server::diagnostics::NamespaceMetadata;
use opcua::server::node_manager::memory::{simple_node_manager, SimpleNodeManager};
use opcua::server::{ServerBuilder, ServerHandle, ServerUserToken, ANONYMOUS_USER_TOKEN_ID};
use opcua::types::{DataValue, MessageSecurityMode, NodeId, StatusCode, UAString, Variant};
use tokio::net::TcpListener;

pub const NAMESPACE: &str = "urn:hmistudio:test";
pub const USER: &str = "operator";
pub const PASSWORD: &str = "operator-password";

fn writable(mut variable: Variable) -> Variable {
    variable.set_writable(true);
    variable.set_user_access_level(
        variable.user_access_level() | opcua::nodes::AccessLevel::CURRENT_WRITE,
    );
    variable
}

pub struct TestServer {
    pub port: u16,
    /// Namespace index of [`NAMESPACE`] on this server.
    pub ns: u16,
    handle: ServerHandle,
    writes: Arc<Mutex<Vec<(String, Variant)>>>,
}

impl TestServer {
    pub fn endpoint(&self) -> String {
        format!("opc.tcp://127.0.0.1:{}", self.port)
    }

    /// Text node id of a demo variable, e.g. `node("Demo.Speed")` → `ns=2;s=Demo.Speed`.
    pub fn node(&self, name: &str) -> String {
        format!("ns={};s={}", self.ns, name)
    }

    /// Every write the server has accepted, in order: exactly what "reached the device".
    pub fn writes(&self) -> Vec<(String, Variant)> {
        self.writes.lock().unwrap().clone()
    }

    pub fn stop(&self) {
        self.handle.cancel();
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.cancel();
    }
}

/// Starts the server on `port` (0 picks a free port). Returns once it is listening.
pub async fn start(port: u16, pki_dir: PathBuf) -> Result<TestServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("cannot bind: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let users = [ANONYMOUS_USER_TOKEN_ID, USER];

    let (server, handle) = ServerBuilder::new()
        .application_name("HMI Studio test server")
        .application_uri("urn:hmistudio:test-server")
        .product_uri("urn:hmistudio:test-server")
        .create_sample_keypair(true)
        .pki_dir(pki_dir)
        .trust_client_certs(true)
        .host("127.0.0.1")
        .port(port)
        .discovery_urls(vec![format!("opc.tcp://127.0.0.1:{port}")])
        .add_user_token(USER, ServerUserToken::user_pass(USER, PASSWORD))
        .add_endpoint(
            "none",
            (
                "/",
                SecurityPolicy::None,
                MessageSecurityMode::None,
                &users as &[&str],
            ),
        )
        .add_endpoint(
            "secure",
            (
                "/",
                SecurityPolicy::Basic256Sha256,
                MessageSecurityMode::SignAndEncrypt,
                &users as &[&str],
            ),
        )
        .with_node_manager(simple_node_manager(
            NamespaceMetadata {
                namespace_uri: NAMESPACE.to_owned(),
                ..Default::default()
            },
            "demo",
        ))
        .build()?;

    let manager = handle
        .node_managers()
        .get_of_type::<SimpleNodeManager>()
        .ok_or("no node manager")?;
    let ns = handle
        .get_namespace_index(NAMESPACE)
        .ok_or("namespace not registered")?;
    let subscriptions = handle.subscriptions().clone();
    let writes = Arc::new(Mutex::new(Vec::new()));
    let id = |name: &str| NodeId::new(ns, name);

    {
        let address_space = manager.address_space();
        let mut address_space = address_space.write();
        let folder = id("Demo");
        address_space.add_folder(&folder, "Demo", "Demo", &NodeId::objects_folder_id());
        let _ = address_space.add_variables(
            vec![
                writable(Variable::new(
                    &id("Demo.Running"),
                    "Running",
                    "Running",
                    false,
                )),
                Variable::new(&id("Demo.Counter"), "Counter", "Counter", 0_i32),
                writable(Variable::new(&id("Demo.Speed"), "Speed", "Speed", 0f32)),
                writable(Variable::new(
                    &id("Demo.Setpoint"),
                    "Setpoint",
                    "Setpoint",
                    0_i16,
                )),
                writable(Variable::new(
                    &id("Demo.Name"),
                    "Name",
                    "Name",
                    UAString::from("line-1"),
                )),
                Variable::new(&id("Demo.Locked"), "Locked", "Locked", 1.5f64),
            ],
            &folder,
        );
    }

    // Writable nodes: record the write, store it and notify subscribers, like a device would.
    for name in ["Demo.Running", "Demo.Speed", "Demo.Setpoint", "Demo.Name"] {
        let node = id(name);
        let writes = writes.clone();
        let manager_for_write = manager.clone();
        let subscriptions = subscriptions.clone();
        let node_for_write = node.clone();
        manager
            .inner()
            .add_write_callback(node, move |value: DataValue, _| {
                let Some(variant) = value.value.clone() else {
                    return StatusCode::BadTypeMismatch;
                };
                writes
                    .lock()
                    .unwrap()
                    .push((name.to_owned(), variant.clone()));
                // Applied after the write call returns: the address space is locked while it runs.
                let manager = manager_for_write.clone();
                let subscriptions = subscriptions.clone();
                let node = node_for_write.clone();
                tokio::spawn(async move {
                    let _ =
                        manager.set_value(&subscriptions, &node, None, DataValue::new_now(variant));
                });
                StatusCode::Good
            });
    }

    let counter = AtomicI32::new(0);
    let counter_node = id("Demo.Counter");
    let token = handle.token().clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        while !token.is_cancelled() {
            interval.tick().await;
            let next = counter.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = manager.set_value(
                &subscriptions,
                &counter_node,
                None,
                DataValue::new_now(next),
            );
        }
    });

    tokio::spawn(server.run_with(listener));
    Ok(TestServer {
        port,
        ns,
        handle,
        writes,
    })
}
