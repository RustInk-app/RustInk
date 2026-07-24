use rusqlite::{Connection, params};
use crate::save_handler::db::*;
use crate::models::page::*;

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
    let (min_x, max_x, min_y, max_y) = bounding_box(payload);

    // I limiti massimi sicuri per il motore R-Tree di SQLite (32-bit float)
    let rtree_max = f32::MAX as f64;
    let rtree_min = f32::MIN as f64;

    // --- SANITIZZAZIONE ANTI-CHAOS ---
    if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite()
        || min_x > max_x || min_y > max_y
        || min_x < rtree_min || max_x > rtree_max
        || min_y < rtree_min || max_y > rtree_max
    {
        return Err(rusqlite::Error::ToSqlConversionFailure(
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData, 
                "Coordinate non valide o fuori dai limiti R-Tree rilevate nel componente"
            ))
        ));
    }

    let bytes = encode_payload(payload);
    conn.execute(
        "INSERT INTO active_components (page_id, is_active, payload) VALUES (?1, 1, ?2)",
        params![page_id, bytes],
    )?;

    let new_id = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO component_rtree (id, minX, maxX, minY, maxY)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![new_id, min_x, max_x, min_y, max_y],
    )?;

    Ok(new_id)
}

pub fn update_bookmark_status(conn: &Connection, page_id: i64, is_bookmarked: bool, name: Option<&str>) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE pages SET is_bookmarked = ?1, bookmark_name = ?2 WHERE id = ?3",
        params![is_bookmarked as i64, name, page_id],
    )?;
    Ok(())
}

// Move a page from old_order to new_order
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