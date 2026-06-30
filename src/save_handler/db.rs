use std::io::{self, Read, Write};
use std::path::Path;

use rusqlite::{Connection, params};
use zip::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

use crate::page::*;

use crate::save_handler::autosave;

const DB_ENTRY: &str = "struttura.sqlite";

pub fn encode_payload(payload: &ComponentPayload) -> Vec<u8> {
    bincode::serialize(payload).expect("bincode serialize ComponentPayload")
}

pub fn decode_payload(bytes: &[u8]) -> io::Result<ComponentPayload> {
    bincode::deserialize(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn encode_payload_list(list: &[ComponentPayload]) -> Vec<u8> {
    bincode::serialize(list).expect("bincode serialize Vec<ComponentPayload>")
}

pub fn decode_payload_list(bytes: &[u8]) -> io::Result<Vec<ComponentPayload>> {
    bincode::deserialize(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn bounding_box(payload: &ComponentPayload) -> (f64, f64, f64, f64) {
    match payload {
        ComponentPayload::PenStroke(stroke) | ComponentPayload::EraserStroke(stroke) => {
            if stroke.points.is_empty() {
                return (0.0, 0.0, 0.0, 0.0);
            }
            let half_w = stroke.width / 2.0;
            let mut min_x = f64::MAX;
            let mut max_x = f64::MIN;
            let mut min_y = f64::MAX;
            let mut max_y = f64::MIN;
            for &(x, y) in &stroke.points {
                if x - half_w < min_x {
                    min_x = x - half_w;
                }
                if x + half_w > max_x {
                    max_x = x + half_w;
                }
                if y - half_w < min_y {
                    min_y = y - half_w;
                }
                if y + half_w > max_y {
                    max_y = y + half_w;
                }
            }
            (min_x, max_x, min_y, max_y)
        }
        ComponentPayload::RichText(block) => block.approx_bbox(),
        ComponentPayload::Image(block) => {
            let w = if block.width > 0.0 {
                block.width
            } else {
                150.0
            };
            let h = if block.height > 0.0 {
                block.height
            } else {
                100.0
            };
            (block.x, block.x + w, block.y, block.y + h)
        }
        ComponentPayload::Shape(block) => {
            let half_w = block.width / 2.0;
            (
                block.x1.min(block.x2) - half_w,
                block.x1.max(block.x2) + half_w,
                block.y1.min(block.y2) - half_w,
                block.y1.max(block.y2) + half_w,
            )
        }
    }
}

pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous   = NORMAL;
        PRAGMA foreign_keys  = ON;

        CREATE TABLE IF NOT EXISTS pages (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            display_order INTEGER NOT NULL,
            created_at    TEXT    NOT NULL DEFAULT (datetime('now')),
            background    INTEGER NOT NULL DEFAULT 0
        );

        -- Each stroke is not stored as a point but as a BLOB 
        -- (think of it as an array) of bytes.
        
        CREATE TABLE IF NOT EXISTS base_layers (
            page_id    INTEGER PRIMARY KEY REFERENCES pages(id) ON DELETE CASCADE,
            baked_blob BLOB NOT NULL
        );

        -- Each component can be active or not. If it's visible on the screen, it's active; otherwise, it's not. 
        -- Use case: If the user presses CTRL+Z on something they've just inserted, it's no longer visible.
        CREATE TABLE IF NOT EXISTS active_components (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            page_id   INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
            is_active INTEGER NOT NULL DEFAULT 1,
            payload   BLOB    NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ac_page
            ON active_components(page_id, is_active);

        CREATE VIRTUAL TABLE IF NOT EXISTS component_rtree
            USING rtree(id, minX, maxX, minY, maxY);
    ",
    )?;

    Ok(())
}

pub fn update_page_background(
    conn: &Connection,
    page_id: i64,
    bg: &PaperBackground,
) -> rusqlite::Result<()> {
    let bg_int = match bg {
        PaperBackground::Ruled => 0,
        PaperBackground::Plain => 1,
        PaperBackground::Grid => 2,
    };
    conn.execute(
        "UPDATE pages SET background = ?1 WHERE id = ?2",
        params![bg_int, page_id],
    )?;
    Ok(())
}

pub fn page_count(conn: &Connection) -> rusqlite::Result<usize> {
    conn.query_row("SELECT COUNT(*) FROM pages", [], |r| r.get::<_, i64>(0))
        .map(|n| n as usize)
}

pub fn page_id_at(conn: &Connection, order_index: usize) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT id FROM pages ORDER BY display_order ASC LIMIT 1 OFFSET ?1",
        params![order_index as i64],
        |r| r.get(0),
    )
}

pub fn insert_page_after(
    conn: &Connection,
    after_order: usize,
    bg: &PaperBackground,
) -> rusqlite::Result<i64> {
    let bg_int = match bg {
        PaperBackground::Ruled => 0,
        PaperBackground::Plain => 1,
        PaperBackground::Grid => 2,
    };
    conn.execute(
        "UPDATE pages SET display_order = display_order + 1 WHERE display_order > ?1",
        params![after_order as i64],
    )?;
    conn.execute(
        "INSERT INTO pages (display_order, background) VALUES (?1, ?2)",
        params![(after_order + 1) as i64, bg_int],
    )?;
    let new_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)",
        params![new_id, encode_payload_list(&[])],
    )?;
    Ok(new_id)
}

pub fn insert_page_before(
    conn: &Connection,
    before_order: usize,
    bg: &PaperBackground,
) -> rusqlite::Result<i64> {
    let bg_int = match bg {
        PaperBackground::Ruled => 0,
        PaperBackground::Plain => 1,
        PaperBackground::Grid => 2,
    };
    conn.execute(
        "UPDATE pages SET display_order = display_order + 1 WHERE display_order >= ?1",
        params![before_order as i64],
    )?;
    conn.execute(
        "INSERT INTO pages (display_order, background) VALUES (?1, ?2)",
        params![before_order as i64, bg_int],
    )?;
    let new_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)",
        params![new_id, encode_payload_list(&[])],
    )?;
    Ok(new_id)
}

pub fn delete_page(conn: &Connection, page_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM component_rtree
         WHERE id IN (
             SELECT id FROM active_components WHERE page_id = ?1
         )",
        params![page_id],
    )?;

    let order: i64 = conn.query_row(
        "SELECT display_order FROM pages WHERE id = ?1",
        params![page_id],
        |r| r.get(0),
    )?;
    conn.execute("DELETE FROM pages WHERE id = ?1", params![page_id])?;
    conn.execute(
        "UPDATE pages SET display_order = display_order - 1 WHERE display_order > ?1",
        params![order],
    )?;
    Ok(())
}

pub fn append_active_component(
    conn: &Connection,
    page_id: i64,
    payload: &ComponentPayload,
) -> rusqlite::Result<i64> {
    let bytes = encode_payload(payload);

    conn.execute(
        "INSERT INTO active_components (page_id, is_active, payload) VALUES (?1, 1, ?2)",
        params![page_id, bytes],
    )?;
    let new_id = conn.last_insert_rowid();

    let (min_x, max_x, min_y, max_y) = bounding_box(payload);
    conn.execute(
        "INSERT INTO component_rtree (id, minX, maxX, minY, maxY)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![new_id, min_x, max_x, min_y, max_y],
    )?;

    Ok(new_id)
}

pub fn toggle_active_state(
    conn: &Connection,
    component_id: i64,
    is_active: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE active_components SET is_active = ?1 WHERE id = ?2",
        params![is_active as i64, component_id],
    )?;
    Ok(())
}

pub fn load_page_full(
    conn: &Connection,
    page_id: i64,
) -> rusqlite::Result<(PageData, Vec<(i64, ComponentPayload)>)> {
    let bg_int: i64 = conn
        .query_row(
            "SELECT background FROM pages WHERE id = ?1",
            params![page_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let background = match bg_int {
        1 => PaperBackground::Plain,
        2 => PaperBackground::Grid,
        _ => PaperBackground::Ruled,
    };

    let baked: Vec<ComponentPayload> = conn
        .query_row(
            "SELECT baked_blob FROM base_layers WHERE page_id = ?1",
            params![page_id],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .ok()
        .and_then(|bytes| decode_payload_list(&bytes).ok())
        .unwrap_or_default();

    let mut stmt = conn.prepare(
        "SELECT id, payload, is_active FROM active_components
         WHERE page_id = ?1
         ORDER BY id ASC",
    )?;

    let all_components: Vec<(i64, ComponentPayload, bool)> = stmt
        .query_map(params![page_id], |row| {
            let id: i64 = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            let is_active: i64 = row.get(2)?;
            Ok((id, bytes, is_active != 0))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(id, bytes, active)| decode_payload(&bytes).ok().map(|p| (id, p, active)))
        .collect();

    let mut components: Vec<ComponentPayload> = baked;
    for (_, payload, is_active) in &all_components {
        if *is_active {
            components.push(payload.clone());
        }
    }

    let active_only: Vec<(i64, ComponentPayload)> = all_components
        .into_iter()
        .filter(|(_, _, a)| *a)
        .map(|(id, p, _)| (id, p))
        .collect();

    Ok((
        PageData {
            components,
            background,
        },
        active_only,
    ))
}

pub fn load_page(conn: &Connection, page_id: i64) -> rusqlite::Result<PageData> {
    load_page_full(conn, page_id).map(|(pd, _)| pd)
}

pub fn get_active_referenced_media(
    conn: &Connection,
) -> rusqlite::Result<std::collections::HashSet<String>> {
    let mut media = std::collections::HashSet::new();

    let mut stmt = conn.prepare("SELECT baked_blob FROM base_layers")?;
    let baked_iter = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
    for blob in baked_iter.flatten() {
        if let Ok(components) = decode_payload_list(&blob) {
            for comp in components {
                if let ComponentPayload::Image(block) = comp {
                    media.insert(block.filename.clone());
                }
            }
        }
    }

    let mut stmt = conn.prepare("SELECT payload FROM active_components WHERE is_active = 1")?;
    let active_iter = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
    for blob in active_iter.flatten() {
        if let Ok(comp) = decode_payload(&blob) {
            if let ComponentPayload::Image(block) = comp {
                media.insert(block.filename.clone());
            }
        }
    }

    Ok(media)
}

pub fn move_page(conn: &Connection, page_id: i64, new_order: usize) -> rusqlite::Result<()> {
    let old_order: i64 = conn.query_row(
        "SELECT display_order FROM pages WHERE id = ?1",
        params![page_id],
        |r| r.get(0),
    )?;

    let new_ord = new_order as i64;
    if old_order == new_ord {
        return Ok(());
    }

    if old_order < new_ord {
        conn.execute(
            "UPDATE pages SET display_order = display_order - 1 
             WHERE display_order > ?1 AND display_order <= ?2",
            params![old_order, new_ord],
        )?;
    } else {
        conn.execute(
            "UPDATE pages SET display_order = display_order + 1 
             WHERE display_order >= ?1 AND display_order < ?2",
            params![new_ord, old_order],
        )?;
    }

    conn.execute(
        "UPDATE pages SET display_order = ?1 WHERE id = ?2",
        params![new_ord, page_id],
    )?;

    Ok(())
}

pub fn export_bundle_path(db_path: &Path, bundle_path: &Path) -> io::Result<()> {
    let db_bytes = std::fs::read(db_path)?;
    eprintln!(
        "[DB] export_bundle_path: {} byte da {:?}",
        db_bytes.len(),
        db_path
    );

    let file = std::fs::File::create(bundle_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file(DB_ENTRY, options)?;
    zip.write_all(&db_bytes)?;

    let mut referenced_media = std::collections::HashSet::new();
    if let Ok(conn) =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    {
        if let Ok(media) = get_active_referenced_media(&conn) {
            referenced_media = media;
        }
    }

    if let Ok(entries) = std::fs::read_dir(&autosave::media_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("webp") {
                let fname = path.file_name().unwrap_or_default().to_string_lossy();
                let entry_name = format!("media/{}", fname);

                if referenced_media.contains(&entry_name) {
                    if let Ok(bytes) = std::fs::read(&path) {
                        eprintln!(
                            "[DB] Aggiunge al bundle: {} ({} byte)",
                            entry_name,
                            bytes.len()
                        );
                        zip.start_file(&entry_name, options)?;
                        zip.write_all(&bytes)?;
                    }
                } else {
                    eprintln!("[DB] Ignoro immagine orfana/annullata: {}", entry_name);
                }
            }
        }
    }

    zip.finish()?;
    Ok(())
}

pub fn import_bundle(bundle_path: &Path, dest_path: &Path) -> io::Result<()> {
    let file = std::fs::File::open(bundle_path)?;
    let mut zip =
        ZipArchive::new(file).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    {
        let mut entry = zip
            .by_name(DB_ENTRY)
            .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        std::fs::write(dest_path, buf)?;
    }

    let media_tmp = autosave::media_dir();

    let _ = std::fs::remove_dir_all(&media_tmp);

    let _ = std::fs::create_dir_all(&media_tmp);

    let mut media_names: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index(i) {
            let name = entry.name().to_owned();
            if name.starts_with("media/") && name.ends_with(".webp") {
                media_names.push(name);
            }
        }
    }

    for name in media_names {
        if let Ok(mut entry) = zip.by_name(&name) {
            let fname = std::path::Path::new(&name)
                .file_name()
                .unwrap_or_default()
                .to_owned();
            let dest = media_tmp.join(&fname);
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            std::fs::write(dest, buf)?;
            eprintln!("[DB] Estratto media: {}", name);
        }
    }

    Ok(())
}
