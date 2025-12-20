use age::x25519;
use hometree_core::config::{BackupPolicy, SecretRule, SecretsConfig};
use hometree_core::{AgeBackend, Config, Paths, SecretsBackend, SecretsManager};
use secrecy::ExposeSecret;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn make_secrets_config(temp: &TempDir) -> (SecretsConfig, PathBuf) {
    let identity = x25519::Identity::generate();
    let identity_str = identity.to_string();
    let identity_path = temp.path().join("age-identity.txt");
    fs::write(&identity_path, identity_str.expose_secret()).unwrap();

    let recipient = identity.to_public().to_string();

    let rule = SecretRule {
        path: ".config/app/secret.env".to_string(),
        ciphertext: None,
        mode: None,
    };

    let config = SecretsConfig {
        enabled: true,
        backend: "age".to_string(),
        sidecar_suffix: ".age".to_string(),
        recipients: vec![recipient],
        identity_files: vec![identity_path.clone()],
        rules: vec![rule],
        backup_policy: BackupPolicy::Encrypt,
    };

    (config, identity_path)
}

#[test]
fn age_backend_encrypts_and_decrypts_round_trip() {
    let temp = TempDir::new().unwrap();
    let (secrets_cfg, _identity_path) = make_secrets_config(&temp);

    let backend = AgeBackend::from_config(&secrets_cfg).expect("backend from config");
    let plaintext = b"super secret value";

    let ciphertext = backend.encrypt(plaintext).expect("encrypt");
    assert_ne!(ciphertext, plaintext);

    let decrypted = backend.decrypt(&ciphertext).expect("decrypt");
    assert_eq!(decrypted, plaintext);
}

#[test]
fn secrets_manager_maps_plaintext_and_ciphertext_paths() {
    let temp = TempDir::new().unwrap();
    let (secrets_cfg, _identity_path) = make_secrets_config(&temp);

    let manager = SecretsManager::from_config(&secrets_cfg);
    assert!(manager.enabled());
    assert!(matches!(manager.backup_policy(), BackupPolicy::Encrypt));

    let rule = manager
        .rules()
        .iter()
        .find(|r| r.path == ".config/app/secret.env")
        .expect("secret rule present");

    let plaintext_rel = manager.plaintext_path(rule);
    let ciphertext_rel = manager.ciphertext_path(rule);

    assert_eq!(plaintext_rel, PathBuf::from(".config/app/secret.env"));
    assert_eq!(ciphertext_rel, PathBuf::from(".config/app/secret.env.age"));

    assert!(manager.is_secret_plaintext(&plaintext_rel));
    assert!(manager.is_ciphertext_path(&ciphertext_rel));
    assert!(!manager.is_ciphertext_path(&plaintext_rel));
}

#[test]
fn secrets_config_defaults_add_ignore_rules_and_sidecar_suffix() {
    let temp = TempDir::new().unwrap();
    let home_root = temp.path().join("home");
    let xdg_root = temp.path().join("xdg");
    fs::create_dir_all(&home_root).unwrap();
    fs::create_dir_all(&xdg_root).unwrap();

    let paths = Paths::new_with_overrides(Some(&home_root), Some(&xdg_root)).unwrap();
    let mut config = Config::default_with_paths(&paths);

    let (mut secrets_cfg, identity_path) = make_secrets_config(&temp);
    // Exercise sidecar_suffix defaulting logic
    secrets_cfg.sidecar_suffix = "".to_string();
    secrets_cfg.identity_files = vec![identity_path];

    config.secrets = secrets_cfg;

    let config_path = paths.config_file();
    config.write_to(&config_path).unwrap();

    let loaded = Config::load_from(&config_path).expect("load config");
    assert!(loaded.secrets.enabled);
    assert_eq!(loaded.secrets.sidecar_suffix, ".age");

    // Secret rules should be mirrored into ignore patterns
    assert!(loaded
        .ignore
        .patterns
        .iter()
        .any(|p| p == ".config/app/secret.env"));
}
