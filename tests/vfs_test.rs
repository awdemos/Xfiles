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
    assert!(vfs.exists("/net/agent-01/ctl/status"));
    assert!(vfs.exists("/net/agent-01/msg/inbox"));
    assert!(vfs.exists("/net/agent-01/msg/outbox"));
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
