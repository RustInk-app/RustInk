use std::sync::mpsc;
use tempfile::tempdir;
use lopdf::Document;

use RASTIN::export::export_document_to_pdf;
use RASTIN::save_handler::db::{init_schema, encode_payload_list};
use RASTIN::save_handler::database_utilities::insert_page_after;
use RASTIN::models::page::PaperBackground;

#[test]
fn test_export_pdf_contains_correct_pages() {
    
    // 1. Prepariamo una cartella temporanea per DB e PDF esportato
    let dir = tempdir().expect("Impossibile creare la cartella temporanea");
    let db_path = dir.path().join("export_test.sqlite");
    let pdf_path = dir.path().join("output_test.pdf");

    // 2. Creiamo il DB e inseriamo 3 pagine
    let conn = rusqlite::Connection::open(&db_path).expect("Impossibile aprire il DB");
    init_schema(&conn).expect("Impossibile inizializzare lo schema");
    
    // Prima pagina (ordine 0)
    conn.execute("INSERT INTO pages (display_order) VALUES (0)", [])
        .expect("Impossibile inserire prima pagina");
    let first_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)",
        rusqlite::params![first_id, encode_payload_list(&[])],
    ).unwrap();

    // Aggiungiamo altre 2 pagine (per un totale di 3) assegnando sfondi diversi,
    // anche se sappiamo che Cairo per ora li esporterà bianchi.
    insert_page_after(&conn, 0, &PaperBackground::Ruled).unwrap();
    insert_page_after(&conn, 1, &PaperBackground::Grid).unwrap();
    
    // Chiudiamo la connessione perché export_document_to_pdf aprirà la sua
    drop(conn);

    // 3. Eseguiamo l'export
    let (tx, rx) = mpsc::channel();
    
    // Lanciamo l'export in un thread per simulare il comportamento della GUI
    let db_path_clone = db_path.clone();
    let pdf_path_clone = pdf_path.clone();
    std::thread::spawn(move || {
        export_document_to_pdf(&db_path_clone, &pdf_path_clone, tx);
    });

    // Ascoltiamo i messaggi dal channel finché non finisce
    let mut success = false;
    while let Ok(msg) = rx.recv() {
        match msg {
            Ok(None) => {
                success = true; // Export terminato con successo!
                break;
            }
            Ok(Some((cur, tot))) => {
                // Progresso in corso, ignoriamo
            }
            Err(e) => panic!("L'esportazione ha riportato un errore: {}", e),
        }
    }
    assert!(success, "L'esportazione non si è conclusa correttamente");

    // 4. VERIFICA: Riapriamo il PDF appena generato e analizziamone l'albero
    assert!(pdf_path.exists(), "Il file PDF non è stato creato sul disco");
    
    let doc = Document::load(&pdf_path).expect("Impossibile leggere il PDF generato. File corrotto?");
    let pages = doc.get_pages();
    
    assert_eq!(
        pages.len(), 
        3, 
        "Il PDF generato dovrebbe contenere esattamente 3 pagine"
    );
}