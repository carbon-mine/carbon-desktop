use super::*;

const CLIENT: Uuid = Uuid::from_u128(1);
const FOREST: Uuid = Uuid::from_u128(2);
const COAST: Uuid = Uuid::from_u128(3);

fn forest() -> SavedProfile {
    SavedProfile {
        id: FOREST,
        name: "Forest".into(),
    }
}
fn coast() -> SavedProfile {
    SavedProfile {
        id: COAST,
        name: "Coast".into(),
    }
}

#[test]
fn registry_survives_reopen_without_credentials_and_cas_prevents_stale_updates() {
    let root = tempfile::tempdir().unwrap();
    let fake = MemoryCredentials::default();
    let vault = fake.vault(root.path(), CLIENT);
    vault.add(forest(), "forest-first").unwrap();
    vault.add(coast(), "coast-first").unwrap();
    vault.select(FOREST).unwrap();
    drop(vault);

    let vault = fake.vault(root.path(), CLIENT);
    let metadata = vault.load().unwrap();
    assert_eq!(metadata.active, Some(FOREST));
    assert_eq!(metadata.accounts, [forest(), coast()]);
    let serialized = fs::read_to_string(&vault.lock().unwrap().registry).unwrap();
    assert!(!serialized.contains("first"));
    assert_eq!(
        vault
            .update(
                forest(),
                "forest-first",
                "forest-rotated",
                true,
                Some(FOREST)
            )
            .unwrap()
            .unwrap()
            .active,
        Some(FOREST),
    );
    assert!(vault
        .update(forest(), "forest-first", "stale-write", true, None)
        .unwrap()
        .is_none());
    assert!(vault
        .remove(FOREST, Some("forest-first"))
        .unwrap()
        .is_none());
    vault.remove(FOREST, None).unwrap();
    assert_eq!(vault.load().unwrap().active, Some(COAST));
    assert!(vault.read(FOREST).unwrap().is_none());
    assert_eq!(vault.read(COAST).unwrap().unwrap().as_str(), "coast-first");
}

#[test]
fn isolated_roots_and_app_registrations_do_not_share_a_slot() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let fake = MemoryCredentials::default();
    let vault = fake.vault(first.path(), CLIENT);
    vault.add(forest(), "one").unwrap();
    let other_app = fake.vault(first.path(), Uuid::from_u128(5));
    let other_root = fake.vault(second.path(), CLIENT);
    for other in [&other_app, &other_root] {
        assert!(other.load().unwrap().accounts.is_empty());
        assert!(other.read(FOREST).unwrap().is_none());
    }
    other_app.add(forest(), "other-app").unwrap();
    other_root.add(forest(), "other-root").unwrap();
    assert_eq!(vault.read(FOREST).unwrap().unwrap().as_str(), "one");
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
#[ignore = "requires a desktop OS credential vault"]
fn native_vault_compare_replace_and_remove() {
    struct Cleanup<'a>(&'a Vault);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            for id in [FOREST, COAST] {
                let _ = self.0.remove(id, None);
            }
        }
    }
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(root.path(), CLIENT);
    let _cleanup = Cleanup(&vault);
    let forest_token = "synthetic".repeat(222);
    let coast_token = "unrelated".repeat(200);
    let next = "replacement".repeat(182);
    vault.add(forest(), &forest_token).unwrap();
    vault.add(coast(), &coast_token).unwrap();
    assert!(vault
        .update(forest(), &forest_token, &next, true, None)
        .unwrap()
        .is_some());
    assert!(vault.remove(FOREST, Some(&forest_token)).unwrap().is_none());
    vault.remove(FOREST, Some(&next)).unwrap().unwrap();
    assert!(vault.read(FOREST).unwrap().is_none());
    assert_eq!(
        vault
            .read(COAST)
            .unwrap()
            .map(|secret| secret.as_str() == coast_token),
        Some(true)
    );
    assert!(fs::read(root.path().join("account-vault.lock"))
        .unwrap()
        .is_empty());
}
