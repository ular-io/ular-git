use super::*;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn transaction_child() {
    let Some(path) = std::env::var_os("ULAR_GIT_TRANSACTION_TEST_PATH") else {
        return;
    };
    let prefix = std::env::var("ULAR_GIT_TRANSACTION_TEST_PREFIX").unwrap();
    for i in 0..12 {
        RepoStore::update(Path::new(&path), true, |store| {
            store
                .add_group(&format!("{prefix}-{i}"))
                .map_err(anyhow::Error::msg)
        })
        .unwrap();
    }
}

#[test]
fn competing_processes_keep_every_operation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    let mut children = Vec::new();
    for prefix in ["a", "b", "c"] {
        children.push(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "store::shared::tests::transaction_child",
                    "--quiet",
                ])
                .env("ULAR_GIT_TRANSACTION_TEST_PATH", &path)
                .env("ULAR_GIT_TRANSACTION_TEST_PREFIX", prefix)
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    let store = RepoStore::read_existing(&path).unwrap();
    assert_eq!(store.groups.len(), 36);
    for prefix in ["a", "b", "c"] {
        for i in 0..12 {
            assert!(store.group_exists(&format!("{prefix}-{i}")));
        }
    }
}

#[test]
fn failure_does_not_write_partial_changes_or_recreate_deleted_lists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    RepoStore::default().save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(RepoStore::update(&path, false, |store| {
        store.add_group("partial").unwrap();
        Err::<(), _>(anyhow::anyhow!("Conflict"))
    })
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::write(&path, "{").unwrap();
    assert!(RepoStore::update(&path, false, |_| Ok(())).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");
    std::fs::remove_file(&path).unwrap();
    assert!(RepoStore::update(&path, false, |_| Ok(())).is_err());
    assert!(!path.exists());
}

#[test]
fn directory_watch_survives_replacement_and_ignores_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.json");
    RepoStore::default().save(&path).unwrap();
    let watcher = StoreWatcher::new(&path).unwrap();
    for name in ["first", "second"] {
        RepoStore::update(&path, false, |store| {
            store.add_group(name).map_err(anyhow::Error::msg)
        })
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while !watcher.changed().unwrap() {
            assert!(
                Instant::now() < deadline,
                "replacement notification missing"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    // Drain delayed write events before verifying that reads stay silent.
    std::thread::sleep(Duration::from_secs(1));
    watcher.changed().unwrap();
    RepoStore::read_existing(&path).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    assert!(!watcher.changed().unwrap());
}

#[test]
fn lock_wait_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repos.lock");
    let owner = std::fs::File::create(&path).unwrap();
    owner.lock().unwrap();
    let contender = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let start = Instant::now();
    let error = acquire_lock(&contender, Duration::from_millis(30)).unwrap_err();
    assert!(error.to_string().contains("Timed out"));
    assert!(start.elapsed() < Duration::from_secs(2));
}
