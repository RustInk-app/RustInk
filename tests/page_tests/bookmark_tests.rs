use rusqlite::Connection;
use RASTIN::save_handler::db::*;
use RASTIN::save_handler::database_utilities::*;
use RASTIN::models::page::PaperBackground;
use RASTIN::gui::state::AppState; 

#[test]
fn test_bookmarks_persistence_and_search() {
    println!("[TEST] 1. Inizializzazione DB in memoria...");
    let conn = Connection::open_in_memory().expect("Impossibile aprire il DB in memoria");
    init_schema(&conn).expect("Impossibile inizializzare lo schema");

    
    conn.execute("INSERT INTO pages (display_order) VALUES (0)", [])
        .expect("Impossibile inserire la prima pagina");
    let first_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)",
        rusqlite::params![first_id, encode_payload_list(&[])],
    ).unwrap();

    println!("[TEST] 2. Creazione di altre 9 pagine (totale 10)...");
    let bg = PaperBackground::Grid;
    for i in 1..10 {
        insert_page_after(&conn, i - 1, &bg).expect("Impossibile inserire pagina");
    }
    assert_eq!(page_count(&conn).unwrap(), 10, "Devono esserci esattamente 10 pagine");

    println!("[TEST] 3. Impostazione di 5 segnalibri...");
    
    let bookmarks_data = vec![
        (2, "Appunti di Matematica"),
        (4, "Riassunto Storia"),
        (5, "Esercizi Matematica Avanzata"),
        (7, "Fisica Quantistica"),
        (9, "Da ricordare"),
    ];

    for &(idx, name) in &bookmarks_data {
        let page_id = page_id_at(&conn, idx).unwrap();
        
        update_bookmark_status(&conn, page_id, true, Some(name)).unwrap();
    }

    println!("[TEST] 4. VERIFICA SALVATAGGIO: Lettura grezza dal Database...");
    for i in 0..10 {
        let page_id = page_id_at(&conn, i).unwrap();
        
        let (is_bk, bk_name): (i64, Option<String>) = conn.query_row(
            "SELECT is_bookmarked, bookmark_name FROM pages WHERE id = ?1",
            rusqlite::params![page_id],
            |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();

        let expected_bookmark = bookmarks_data.iter().find(|&&(idx, _)| idx == i);
        if let Some(&(_, expected_name)) = expected_bookmark {
            assert_eq!(is_bk, 1, "La pagina {} doveva essere salvata come segnalibro", i);
            assert_eq!(bk_name.unwrap(), expected_name, "Nome segnalibro salvato errato per la pagina {}", i);
        } else {
            assert_eq!(is_bk, 0, "La pagina {} NON doveva essere salvata come segnalibro", i);
            assert!(bk_name.is_none(), "La pagina {} non doveva avere un nome segnalibro salvato", i);
        }
    }

    println!("[TEST] 5. VERIFICA RICERCA INTERNA: Inizializzazione AppState e Trie...");
    
    
    let mut app_state = AppState::new();
    
    
    app_state.db = Some(conn);
    
    
    app_state.rebuild_bookmark_index();

    assert_eq!(
        app_state.bookmarked_pages.len(), 
        5, 
        "Il motore di ricerca non ha caricato esattamente 5 segnalibri"
    );

    
    let check_search = |query: &str| -> std::collections::HashSet<usize> {
        let mut results = std::collections::HashSet::new();
        let mut is_first = true;
        
        for term in query.to_lowercase().split_whitespace() {
            if let Some(pages) = app_state.bookmark_trie.search(term) {
                if is_first {
                    results = pages.clone();
                    is_first = false;
                } else {
                    
                    results.retain(|idx| pages.contains(idx));
                }
            } else {
                return std::collections::HashSet::new(); 
            }
        }
        results
    };

    println!("[TEST] 6. Esecuzione query di ricerca testuali...");

    
    let res = check_search("Matematica");
    assert!(
        res.contains(&2) && res.contains(&5) && res.len() == 2, 
        "ERRORE: La ricerca 'Matematica' deve trovare esattamente le pagine 2 e 5"
    );

    
    let res = check_search("mate");
    assert!(
        res.contains(&2) && res.contains(&5) && res.len() == 2, 
        "ERRORE: La ricerca parziale 'mate' deve trovare esattamente le pagine 2 e 5"
    );

    
    let res = check_search("avanzata esercizi");
    assert!(
        res.contains(&5) && res.len() == 1, 
        "ERRORE: La ricerca multi-termine 'avanzata esercizi' deve trovare solo la pagina 5"
    );

    
    let res = check_search("chimica");
    assert!(
        res.is_empty(), 
        "ERRORE: La ricerca di 'chimica' non deve restituire alcun risultato"
    );

    println!("[TEST] TEST SUI BOOKMARK COMPLETATO CON SUCCESSO!");
}