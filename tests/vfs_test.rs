use xfiles::fs::{VfsRegistry, Vnode};

#[test]
fn test_vfs_seed_core() {
    let vfs = VfsRegistry::new();

    assert!(vfs.exists("/net"));
    assert!(vfs.exists("/ai"));
    assert!(vfs.exists("/proc"));
    assert!(vfs.exists("/msg"));
    assert!(vfs.exists("/plumber"));
    assert!(vfs.exists("/ctl"));
}

#[test]
fn test_vfs_mount_agent() {
    let vfs = VfsRegistry::new();
    vfs.mount_agent_ns("agent-01", "laptop01");

    assert!(vfs.exists("/net/agent-01"));
    assert!(vfs.exists("/net/agent-01/hostname"));
    assert!(vfs.exists("/net/agent-01/ctl"));
    assert!(vfs.exists("/net/agent-01/ctl/status"));
    assert!(vfs.exists("/net/agent-01/msg"));
    assert!(vfs.exists("/net/agent-01/msg/inbox"));
    assert!(vfs.exists("/net/agent-01/msg/outbox"));
}

#[test]
fn test_unmount_agent_ns_prefix_boundary() {
    let vfs = VfsRegistry::new();
    vfs.mount_agent_ns("agent-1", "host1");
    vfs.mount_agent_ns("agent-10", "host10");

    vfs.unmount_agent_ns("agent-1");

    assert!(!vfs.exists("/net/agent-1"));
    assert!(!vfs.exists("/net/agent-1/hostname"));
    assert!(vfs.exists("/net/agent-10"));
    assert!(vfs.exists("/net/agent-10/hostname"));
    assert!(vfs.exists("/net/agent-10/ctl/status"));

    let children = vfs.list("/net");
    assert!(children.contains(&"agent-10".to_string()));
    assert!(!children.contains(&"agent-1".to_string()));
}

#[test]
fn test_vfs_listing_prefix_boundary() {
    let vfs = VfsRegistry::new();
    vfs.mount_agent_ns("agent-1", "host1");
    vfs.mount_agent_ns("agent-10", "host10");

    let children = vfs.list("/net/agent-1");
    assert!(children.contains(&"hostname".to_string()));
    assert!(children.contains(&"ctl".to_string()));
    assert!(children.contains(&"msg".to_string()));
    assert!(!children.contains(&"0".to_string()));

    let net_children = vfs.list("/net");
    assert_eq!(net_children.len(), 2);
}

#[test]
fn test_remove_unlinks_child_from_parent() {
    let vfs = VfsRegistry::new();
    vfs.mount_agent_ns("agent-1", "host1");

    vfs.remove("/net/agent-1/hostname");

    let children = vfs.list("/net/agent-1");
    assert!(!children.contains(&"hostname".to_string()));

    // Full unmount must not leave phantom entries in the parent's listing
    vfs.unmount_agent_ns("agent-1");
    assert!(!vfs.list("/net").contains(&"agent-1".to_string()));
}

#[tokio::test]
async fn test_dir_read_reflects_unmount() {
    let vfs = VfsRegistry::new();
    vfs.mount_agent_ns("agent-1", "host1");
    vfs.mount_agent_ns("agent-10", "host10");

    vfs.unmount_agent_ns("agent-1");

    let net = vfs.get("/net").unwrap();
    let data = net.read().await;
    let listing = String::from_utf8_lossy(&data);
    let entries: Vec<&str> = listing.lines().collect();
    assert!(!entries.contains(&"agent-1"));
    assert!(entries.contains(&"agent-10"));
}

#[tokio::test]
async fn test_vfs_read_write_file() {
    let vfs = VfsRegistry::new();
    vfs.add_node("/test/file", Vnode::new_file("file", b"hello".to_vec()));

    let node = vfs.get("/test/file").unwrap();
    let data = node.read().await;
    assert_eq!(data, b"hello");

    node.write(b"world".to_vec()).await.unwrap();
    let data = node.read().await;
    assert_eq!(data, b"world");
}

#[tokio::test]
async fn test_vfs_directory_listing() {
    let vfs = VfsRegistry::new();
    vfs.mkdir("/test/dir");
    vfs.add_node("/test/dir/a", Vnode::new_file("a", b"".to_vec()));
    vfs.add_node("/test/dir/b", Vnode::new_file("b", b"".to_vec()));

    let children = vfs.list("/test/dir");
    assert!(children.contains(&"a".to_string()));
    assert!(children.contains(&"b".to_string()));
}
