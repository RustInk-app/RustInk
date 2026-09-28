use fs2::FileExt;
use std::fs::File;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::{Builder, TempDir};
use gtk::prelude::*;

use crate::save_handler::autosave_utilities::*;

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

                if dir_name.starts_with("rustInk-") && p != current_session_path {
                    let lock_path = p.join("session.lock");
                    if let Ok(file) = File::open(&lock_path) {
                        if file.try_lock_exclusive().is_err() {
                            continue;
                        }
                    }

                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    }
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

            if p.is_dir() && dir_name.starts_with("rustInk-") && p != current_session_path {
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
                        if bp.extension().and_then(|e| e.to_str()) == Some("rustInk") {
                            all_backups.push((bp, old_backup_dir.clone()));
                        }
                    }
                }
            }
        }
    }

    if all_backups.is_empty() {
        eprintln!("No previous backup found.");
        clear_old_sessions();
        return None;
    }
    
    all_backups.sort_by_key(|(bp, _)| {
        bp.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });

    let (latest_backup, backup_dir) = all_backups.into_iter().last()?;
    eprintln!("Found last backup: {:?}", latest_backup);

    let sentinel_path = backup_dir.join("last_session.txt");
    let sentinel = std::fs::read_to_string(sentinel_path).ok();
    let original = sentinel
        .filter(|s| !s.trim().is_empty())
        .map(|s| PathBuf::from(s.trim()));

    let dialog = gtk::MessageDialog::builder()
        .message_type(gtk::MessageType::Question)
        .buttons(gtk::ButtonsType::YesNo)
        .text("Recovery session")
        .secondary_text("An unrestored save file was found. Do you want to recover it?")
        .build();

    let response = dialog.run();
    dialog.close();

    if response == gtk::ResponseType::Yes {
        
        Some((latest_backup, original))
    } else {
        clear_old_sessions(); 
        None
    }
}
