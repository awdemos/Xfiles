use crate::fs::vnode::Vnode;
use dashmap::DashMap;
use std::sync::Arc;

/// In-memory registry of all virtual nodes.
#[derive(Debug, Clone, Default)]
pub struct VfsRegistry {
    nodes: Arc<DashMap<String, Vnode>>,
}

impl VfsRegistry {
    pub fn new() -> Self {
        let registry = Self {
            nodes: Arc::new(DashMap::new()),
        };
        // Seed core Plan 9-inspired namespace
        registry.seed_core();
        registry
    }

    fn seed_core(&self) {
        self.mkdir("/net");
        self.mkdir("/ai");
        self.mkdir("/proc");
        self.mkdir("/msg");
        self.mkdir("/plumber");
        self.mkdir("/ctl");

        // Core control files
        self.add_node("/ctl/status", Vnode::new_ctl("status", "ok"));
        self.add_node("/ctl/shutdown", Vnode::new_ctl("shutdown", "write 1 to shutdown"));
        self.add_node("/plumber/rules", Vnode::new_file("rules", b"[]".to_vec()));
    }

    pub fn mkdir(&self, path: &str) {
        let path = normalize(path);
        self.nodes.insert(path.clone(), Vnode::new_dir(&path));
        // Ensure parent knows about this child
        if let Some(parent) = parent_path(&path) {
            self.link_child(parent, &path);
        }
    }

    pub fn add_node(&self, path: &str, node: Vnode) {
        let path = normalize(path);
        self.nodes.insert(path.clone(), node);
        if let Some(parent) = parent_path(&path) {
            self.link_child(parent, &path);
        }
    }

    pub fn get(&self, path: &str) -> Option<Vnode> {
        self.nodes.get(&normalize(path)).map(|e| e.clone())
    }

    pub fn remove(&self, path: &str) -> Option<Vnode> {
        self.nodes.remove(&normalize(path)).map(|(_, v)| v)
    }

    pub fn exists(&self, path: &str) -> bool {
        self.nodes.contains_key(&normalize(path))
    }

    pub fn list(&self, path: &str) -> Vec<String> {
        let path = normalize(path);
        match self.nodes.get(&path) {
            Some(node) if node.is_dir() => {
                // Collect children by prefix matching
                let mut children = Vec::new();
                for entry in self.nodes.iter() {
                    let key = entry.key();
                    if let Some(child) = child_name(&path, key) {
                        children.push(child.to_string());
                    }
                }
                children.sort();
                children.dedup();
                children
            }
            _ => Vec::new(),
        }
    }

    fn link_child(&self, parent: String, child: &str) {
        if let Some(node) = self.nodes.get(&parent) {
            if let Vnode::Dir(ref dir) = *node {
                // Best-effort; don't block on async here
                let child_name = child.rsplit('/').next().unwrap_or(child).to_string();
                if let Ok(mut guard) = dir.children.try_write() {
                    if !guard.contains(&child_name) {
                        guard.push(child_name);
                    }
                }
            }
        }
    }

    pub fn mount_agent_ns(&self, agent_id: &str, hostname: &str) {
        let base = format!("/net/{}", agent_id);
        self.mkdir(&base);
        self.add_node(&format!("{}/hostname", base), Vnode::new_file("hostname", hostname.as_bytes().to_vec()));
        self.add_node(&format!("{}/ctl/status", base), Vnode::new_ctl("status", "connected"));
        self.add_node(&format!("{}/ctl/capabilities", base), Vnode::new_file("capabilities", b"[]".to_vec()));
        self.add_node(&format!("{}/msg/inbox", base), Vnode::new_file("inbox", b"".to_vec()));
        self.add_node(&format!("{}/msg/outbox", base), Vnode::new_file("outbox", b"".to_vec()));
    }

    pub fn unmount_agent_ns(&self, agent_id: &str) {
        let base = format!("/net/{}", agent_id);
        let to_remove: Vec<String> = self
            .nodes
            .iter()
            .filter(|e| e.key().starts_with(&base))
            .map(|e| e.key().clone())
            .collect();
        for key in to_remove {
            self.nodes.remove(&key);
        }
    }
}

fn normalize(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() || path == "/" {
        return "/".into();
    }
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let mut normalized = Vec::new();
    for part in parts {
        match part {
            ".." => {
                normalized.pop();
            }
            "." => {}
            _ => normalized.push(part),
        }
    }
    format!("/{}", normalized.join("/"))
}

fn parent_path(path: &str) -> Option<String> {
    let path = normalize(path);
    if path == "/" {
        return None;
    }
    let idx = path.rfind('/').unwrap_or(0);
    if idx == 0 {
        Some("/".into())
    } else {
        Some(path[..idx].into())
    }
}

fn child_name(parent: &str, full: &str) -> Option<String> {
    let parent = normalize(parent);
    let full = normalize(full);
    if full == parent || !full.starts_with(&parent) {
        return None;
    }
    let remainder = &full[parent.len()..];
    let remainder = remainder.strip_prefix('/').unwrap_or(remainder);
    // Only direct children
    if remainder.contains('/') {
        return None;
    }
    Some(remainder.to_string())
}

