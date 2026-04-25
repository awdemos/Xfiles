use crate::agent::AgentRegistry;
use crate::fs::VfsRegistry;
use crate::message::CapabilityManifest;
use dashmap::DashMap;
use std::sync::Arc;

/// Manages per-agent namespaces in the virtual filesystem.
#[derive(Debug, Clone)]
pub struct NamespaceManager {
    vfs: VfsRegistry,
    #[allow(dead_code)]
    agents: AgentRegistry,
    /// Tracks which agents are bound to which namespaces.
    bindings: Arc<DashMap<String, String>>, // agent_id -> namespace_path
}

impl NamespaceManager {
    pub fn new(vfs: VfsRegistry, agents: AgentRegistry) -> Self {
        Self {
            vfs,
            agents,
            bindings: Arc::new(DashMap::new()),
        }
    }

    pub fn create_namespace(&self, manifest: &CapabilityManifest) -> String {
        let ns = format!("/net/{}", manifest.agent_id);
        self.vfs.mkdir(&ns);
        self.vfs.add_node(
            &format!("{}/hostname", ns),
            crate::fs::vnode::Vnode::new_file("hostname", manifest.hostname.as_bytes().to_vec()),
        );
        self.vfs.add_node(
            &format!("{}/ctl/status", ns),
            crate::fs::vnode::Vnode::new_ctl("status", "connected"),
        );
        self.vfs.add_node(
            &format!("{}/ctl/capabilities", ns),
            crate::fs::vnode::Vnode::new_file(
                "capabilities",
                serde_json::to_vec(&manifest.capabilities).unwrap_or_default(),
            ),
        );
        self.vfs.mkdir(&format!("{}/msg", ns));
        self.vfs.add_node(
            &format!("{}/msg/inbox", ns),
            crate::fs::vnode::Vnode::new_file("inbox", b"".to_vec()),
        );
        self.vfs.add_node(
            &format!("{}/msg/outbox", ns),
            crate::fs::vnode::Vnode::new_file("outbox", b"".to_vec()),
        );

        // Bind capabilities as subdirectories
        for cap in &manifest.capabilities {
            let cap_dir = format!("{}/cap/{}", ns, cap.name);
            self.vfs.mkdir(&cap_dir);
            self.vfs.add_node(
                &format!("{}/version", cap_dir),
                crate::fs::vnode::Vnode::new_file("version", cap.version.as_bytes().to_vec()),
            );
            for path in &cap.paths {
                self.vfs.add_node(
                    &format!("{}/{}", cap_dir, path.trim_start_matches('/')),
                    crate::fs::vnode::Vnode::new_file(path, b"".to_vec()),
                );
            }
        }

        self.bindings.insert(manifest.agent_id.clone(), ns.clone());
        ns
    }

    pub fn remove_namespace(&self, agent_id: &str) {
        if let Some((_, ns)) = self.bindings.remove(agent_id) {
            self.vfs.unmount_agent_ns(agent_id);
            // Also clean up the namespace root if different
            let _ = ns;
        }
        // Also remove all /net entries for this agent
        self.vfs.unmount_agent_ns(agent_id);
    }

    pub fn resolve(&self, agent_id: &str) -> Option<String> {
        self.bindings.get(agent_id).map(|e| e.clone())
    }

    pub fn list_namespaces(&self) -> Vec<(String, String)> {
        self.bindings
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }
}
