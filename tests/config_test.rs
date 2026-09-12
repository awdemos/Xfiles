use std::io::Write;

#[test]
fn env_overrides_apply_on_top_of_config_file() {
    let path =
        std::env::temp_dir().join(format!("xfiles-config-test-{}.toml", uuid::Uuid::new_v4()));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "[hub]").unwrap();
        writeln!(f, "bind_addr = \"127.0.0.1:1111\"").unwrap();
        writeln!(f, "max_agents = 7").unwrap();
        writeln!(f, "[quantum]").unwrap();
        writeln!(f, "enabled = true").unwrap();
    }

    std::env::set_var("XFILES_CONFIG", &path);
    std::env::set_var("XFILES_BIND", "127.0.0.1:2222");
    std::env::set_var("XFILES_MAX_AGENTS", "99");
    std::env::set_var("XFILES_QUANTUM_ENABLED", "false");

    let cfg = xfiles::config::Config::from_env().unwrap();
    assert_eq!(cfg.hub.bind_addr, "127.0.0.1:2222".parse().unwrap());
    assert_eq!(cfg.hub.max_agents, 99);
    assert!(!cfg.quantum.enabled);

    std::env::remove_var("XFILES_BIND");
    std::env::remove_var("XFILES_MAX_AGENTS");
    std::env::remove_var("XFILES_QUANTUM_ENABLED");

    let cfg = xfiles::config::Config::from_env().unwrap();
    assert_eq!(cfg.hub.bind_addr, "127.0.0.1:1111".parse().unwrap());
    assert_eq!(cfg.hub.max_agents, 7);
    assert!(cfg.quantum.enabled);

    std::env::remove_var("XFILES_CONFIG");
    std::fs::remove_file(&path).ok();
}
