use crate::save_handler::db::*;

#[derive(Clone, Debug)]
pub struct PdfDocumentRow {
    pub relative_path: String,
}



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
    conn: &mut rusqlite::Connection, 
    start_order: i64,
    pdf_doc_id: i64,
    n_pages: usize,
) -> rusqlite::Result<i64> {
    
    
    let tx = conn.transaction()?;

    let mut first_new_id = 0;

    
    let empty_blob = encode_payload_list(&[]);

    {
        
        
        let mut stmt_page = tx.prepare(
            "INSERT INTO pages (display_order, pdf_doc_id, pdf_page_index) VALUES (?1, ?2, ?3)"
        )?;
        let mut stmt_layer = tx.prepare(
            "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)"
        )?;

        
        for i in 0..n_pages {
            let display_order = start_order + i as i64;
            let pdf_page_index = i as i64;

            stmt_page.execute(rusqlite::params![display_order, pdf_doc_id, pdf_page_index])?;

            
            let page_id = tx.last_insert_rowid();

            if i == 0 {
                first_new_id = page_id;
            }

            stmt_layer.execute(rusqlite::params![page_id, &empty_blob])?;
        }
    } 

    
    tx.commit()?;

    Ok(first_new_id)
}

