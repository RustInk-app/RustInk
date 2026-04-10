use fs2::FileExt;
use std::fs::File;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::{Builder, TempDir};
use gtk::prelude::*;

use std::sync::mpsc::{channel, Receiver};

use crate::save_handler::db::*;
use crate::save_handler::autosave;

use crate::models::page::*;

use std::path::Path;

pub struct OpenResult {
    pub page_count: usize, 
    pub first_id: i64, 
    pub first_page: PageData, 
    pub conn: rusqlite::Connection, 
    pub bundle_path: Option<PathBuf>, 
    pub tmp: PathBuf,
}

pub static SESSION_TEMP_DIR: LazyLock<TempDir> = LazyLock::new(|| {
    let dir = Builder::new()
        .prefix("rastin-")
        .rand_bytes(8)
        .tempdir()
        .expect("Impossibile creare la cartella temporanea della sessione");

    let _ = std::fs::create_dir_all(dir.path().join("media"));
    let _ = std::fs::create_dir_all(dir.path().join("backup"));

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

pub fn write_autosave_sentinel(bundle_path: Option<&PathBuf>) {
    let content = bundle_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let _ = std::fs::write(autosave_sentinel_path(), content);
}

pub fn clear_old_sessions() {
    let temp_base = std::env::temp_dir();
    let current_session_path = SESSION_TEMP_DIR.path();

    if let Ok(entries) = std::fs::read_dir(temp_base) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let dir_name = p.file_name().unwrap_or_default().to_string_lossy();

                if dir_name.starts_with("rastin-") && p != current_session_path {
                    let lock_path = p.join("session.lock");
                    if let Ok(file) = File::open(&lock_path) {
                        if file.try_lock_exclusive().is_err() {
                            eprintln!("[CLEANUP] Ignoro {:?} perché l'istanza è viva", p);
                            continue;
                        }
                    }

                    eprintln!("[CLEANUP] Rimuovo vecchia cartella di sessione: {:?}", p);
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    }
}

pub fn open_document_in_background(
    chosen: PathBuf, 
    tmp: PathBuf
) -> Receiver<Result<OpenResult, String>> {
    
    let ext = chosen
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();

    let (tx, rx) = channel::<Result<OpenResult, String>>();
    let chosen_clone = chosen.clone();

    std::thread::spawn(move || {
        let result = (|| -> Result<OpenResult, String> {
            
            import_bundle(&chosen_clone, &tmp).map_err(|e| e.to_string())?;
            let conn = rusqlite::Connection::open(&tmp).map_err(|e| e.to_string())?;
            let count    = page_count(&conn).unwrap_or(1);
            let first_id = page_id_at(&conn, 0).unwrap_or(1);
            let first_page = load_page(&conn, first_id).unwrap_or_default();
            Ok(OpenResult { page_count: count, first_id, first_page, conn, bundle_path: Some(chosen_clone), tmp })
        
        })();
        let _ = tx.send(result);
    });

    rx
}

pub fn check_recovery() -> Option<(PathBuf, Option<PathBuf>)> {
    let temp_base = std::env::temp_dir();
    let current_session_path = SESSION_TEMP_DIR.path();
    let mut all_backups = Vec::new();

    eprintln!("[RECOVERY] Cerco backup precedenti in {:?}", temp_base);

    if let Ok(entries) = std::fs::read_dir(temp_base) {
        for entry in entries.flatten() {
            let p = entry.path();
            let dir_name = p.file_name().unwrap_or_default().to_string_lossy();

            if p.is_dir() && dir_name.starts_with("rastin-") && p != current_session_path {
                let lock_path = p.join("session.lock");
                if let Ok(file) = File::open(&lock_path) {
                    if file.try_lock_exclusive().is_err() {
                        continue;
                    }
                }

                let old_backup_dir = p.join("backup");

                if let Ok(backup_entries) = std::fs::read_dir(&old_backup_dir) {
                    for b_entry in backup_entries.flatten() {
                        let bp = b_entry.path();
                        if bp.extension().and_then(|e| e.to_str()) == Some("rastin") {
                            all_backups.push((bp, old_backup_dir.clone()));
                        }
                    }
                }
            }
        }
    }

    if all_backups.is_empty() {
        eprintln!("[RECOVERY] Nessun backup precedente trovato.");
        return None;
    }
    
    all_backups.sort_by_key(|(bp, _)| {
        bp.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });

    let (latest_backup, backup_dir) = all_backups.into_iter().last()?;
    eprintln!("[RECOVERY] Backup più recente trovato: {:?}", latest_backup);

    let sentinel_path = backup_dir.join("last_session.txt");
    let sentinel = std::fs::read_to_string(sentinel_path).ok();
    let original = sentinel
        .filter(|s| !s.trim().is_empty())
        .map(|s| PathBuf::from(s.trim()));

    let dialog = gtk::MessageDialog::builder()
        .message_type(gtk::MessageType::Question)
        .buttons(gtk::ButtonsType::YesNo)
        .text("Recupero Sessione")
        .secondary_text("È stato trovato un salvataggio non ripristinato. Vuoi recuperarlo?")
        .build();

    let response = dialog.run();
    dialog.close();

    if response == gtk::ResponseType::Yes {
        println!("Recupero confermato per: {:?}", latest_backup);
        
        Some((latest_backup, original))
    } else {
        println!("Recupero annullato dall'utente.");
        clear_old_sessions(); 
        None
    }
}
