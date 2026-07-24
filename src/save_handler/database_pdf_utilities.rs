use crate::save_handler::db::*;

#[derive(Clone, Debug)]
pub struct PdfDocumentRow {
    pub relative_path: String,
}


// Utilities sui PDF
pub fn get_pdf_document(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<PdfDocumentRow> {
    conn.query_row(
        "SELECT id, relative_path, original_name, page_count FROM pdf_documents WHERE id = ?1",
        [id],
        |row| {
            Ok(PdfDocumentRow {
                relative_path: row.get(1)?,
            })
        },
    )
}

pub fn get_page_pdf_ref(
    conn: &rusqlite::Connection,
    page_id: i64,
) -> rusqlite::Result<Option<(i64, i64)>> {
    conn.query_row(
        "SELECT pdf_doc_id, pdf_page_index FROM pages WHERE id = ?1",
        [page_id],
        |row| {
            let doc_id: Option<i64> = row.get(0)?;
            let page_idx: Option<i64> = row.get(1)?;
            Ok(doc_id.zip(page_idx))
        },
    )
}

pub fn insert_pdf_backed_pages_bulk(
    conn: &mut rusqlite::Connection, // Deve essere mutabile per avviare una transazione
    start_order: i64,
    pdf_doc_id: i64,
    n_pages: usize,
) -> rusqlite::Result<i64> {
    // 1. Avvia la transazione. Tutti gli insert avverranno in memoria (RAM)
    // e verranno scritti su disco solo al momento del commit.
    let tx = conn.transaction()?;

    let mut first_new_id = 0;

    // 2. Pre-calcoliamo il blob vuoto una volta sola per non serializzarlo ad ogni ciclo
    let empty_blob = encode_payload_list(&[]);

    {
        // 3. Prepariamo gli statement SQL fuori dal ciclo.
        // Questo evita che SQLite debba ri-compilare la query ad ogni iterazione.
        let mut stmt_page = tx.prepare(
            "INSERT INTO pages (display_order, pdf_doc_id, pdf_page_index) VALUES (?1, ?2, ?3)"
        )?;
        let mut stmt_layer = tx.prepare(
            "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)"
        )?;

        // 4. Eseguiamo il ciclo ad altissima velocità
        for i in 0..n_pages {
            let display_order = start_order + i as i64;
            let pdf_page_index = i as i64;

            stmt_page.execute(rusqlite::params![display_order, pdf_doc_id, pdf_page_index])?;

            // Recupera l'ID appena generato
            let page_id = tx.last_insert_rowid();

            if i == 0 {
                first_new_id = page_id;
            }

            stmt_layer.execute(rusqlite::params![page_id, &empty_blob])?;
        }
    } // I prepared statement vengono scartati qui per liberare la transazione

    // 5. Scrive fisicamente tutto su disco in un'unica singola operazione
    tx.commit()?;

    Ok(first_new_id)
}

// fine utilities sui pdf