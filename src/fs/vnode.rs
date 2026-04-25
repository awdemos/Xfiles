use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// A virtual node in the Plan 9-inspired filesystem.
#[derive(Debug, Clone)]
pub enum Vnode {
    Dir(Vdir),
    File(Vfile),
    Ctl(Vctl),
    Symlink(String),
}

/// Virtual directory.
#[derive(Debug, Clone)]
pub struct Vdir {
    pub name: String,
    pub children: Arc<RwLock<Vec<String>>>,
}

/// Virtual file (read/write data).
#[derive(Debug, Clone)]
pub struct Vfile {
    pub name: String,
    pub content: Arc<RwLock<Vec<u8>>>,
}

/// Control file (write triggers action, read returns status).
#[derive(Debug, Clone)]
pub struct Vctl {
    pub name: String,
    pub status: Arc<RwLock<String>>,
}

/// Request to read a virtual file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReadRequest {
    pub path: String,
    pub offset: Option<u64>,
    pub count: Option<u64>,
}

/// Request to write a virtual file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WriteRequest {
    pub path: String,
    pub data: Vec<u8>,
    pub offset: Option<u64>,
}

/// Response from a virtual filesystem operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsResponse {
    pub path: String,
    pub data: Option<Vec<u8>>,
    pub error: Option<String>,
}

/// Helper to identify vnode kinds without full clone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VnodeKind {
    Dir,
    File,
    Ctl,
    Symlink,
}

impl Vnode {
    pub fn kind(&self) -> VnodeKind {
        match self {
            Vnode::Dir(_) => VnodeKind::Dir,
            Vnode::File(_) => VnodeKind::File,
            Vnode::Ctl(_) => VnodeKind::Ctl,
            Vnode::Symlink(_) => VnodeKind::Symlink,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Vnode::Dir(d) => &d.name,
            Vnode::File(f) => &f.name,
            Vnode::Ctl(c) => &c.name,
            Vnode::Symlink(s) => s.as_str(),
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, Vnode::Dir(_))
    }

    pub fn new_dir(name: impl Into<String>) -> Self {
        Vnode::Dir(Vdir {
            name: name.into(),
            children: Arc::new(RwLock::new(Vec::new())),
        })
    }

    pub fn new_file(name: impl Into<String>, content: Vec<u8>) -> Self {
        Vnode::File(Vfile {
            name: name.into(),
            content: Arc::new(RwLock::new(content)),
        })
    }

    pub fn new_ctl(name: impl Into<String>, status: impl Into<String>) -> Self {
        Vnode::Ctl(Vctl {
            name: name.into(),
            status: Arc::new(RwLock::new(status.into())),
        })
    }

    pub async fn read(&self) -> Vec<u8> {
        match self {
            Vnode::File(f) => f.content.read().await.clone(),
            Vnode::Ctl(c) => c.status.read().await.as_bytes().to_vec(),
            Vnode::Dir(d) => {
                let children = d.children.read().await;
                let listing = children.join("\n");
                listing.into_bytes()
            }
            Vnode::Symlink(target) => target.as_bytes().to_vec(),
        }
    }

    pub async fn write(&self, data: Vec<u8>) -> anyhow::Result<()> {
        match self {
            Vnode::File(f) => {
                let mut content = f.content.write().await;
                *content = data;
                Ok(())
            }
            Vnode::Ctl(c) => {
                let text = String::from_utf8_lossy(&data);
                let mut status = c.status.write().await;
                *status = text.to_string();
                Ok(())
            }
            Vnode::Dir(_) => Err(anyhow::anyhow!("cannot write to directory")),
            Vnode::Symlink(_) => Err(anyhow::anyhow!("cannot write to symlink")),
        }
    }
}
