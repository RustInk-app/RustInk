use fs2::FileExt;
use std::fs::File;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::{Builder, TempDir};


pub static SESSION_TEMP_DIR: LazyLock<TempDir> = LazyLock::new(|| {
    let dir = Builder::new()
        .prefix("rastin-")
        .rand_bytes(8)
        .tempdir()
        .expect("Impossibile creare la cartella temporanea della sessione");

    let _ = std::fs::create_dir_all(dir.path().join("media"));
    let _ = std::fs::create_dir_all(dir.path().join("backup"));
    let _ = std::fs::create_dir_all(dir.path().join("docs"));

    let lock_path = dir.path().join("session.lock");
    let lock_file = File::create(lock_path).expect("Impossibile creare lockfile");
    lock_file
        .try_lock_exclusive()
        .expect("Impossibile bloccare la sessione");

    Box::leak(Box::new(lock_file));

    dir
});

pub fn temp_db_dir() -> PathBuf {
    SESSION_TEMP_DIR.path().join("struttura.sqlite")
}

pub fn media_dir() -> PathBuf {
    SESSION_TEMP_DIR.path().join("media")
}

pub fn docs_dir() -> PathBuf {
    SESSION_TEMP_DIR.path().join("docs")
}

pub fn backup_dir() -> PathBuf {
    SESSION_TEMP_DIR.path().join("backup")
}

pub fn autosave_path() -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    backup_dir().join(format!("backup_{ts}.rastin"))
}

pub fn autosave_sentinel_path() -> PathBuf {
    backup_dir().join("last_session.txt")
}
