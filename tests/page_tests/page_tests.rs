use rusqlite::Connection;
use rustInk::save_handler::db::*;
use rustInk::save_handler::database_utilities::*;
use rustInk::models::page::PaperBackground;

#[test]
fn test_pages_backgrounds_and_movement() {
    println!("[TEST] 1. Inizializzazione DB in memoria...");
    let conn = Connection::open_in_memory().expect("Impossibile aprire il DB in memoria");
    init_schema(&conn).expect("Impossibile inizializzare lo schema");

    let backgrounds = vec![
        PaperBackground::Ruled,
        PaperBackground::Plain,
        PaperBackground::Grid,
    ];

    println!("[TEST] 2. Inserimento prima pagina...");
    conn.execute("INSERT INTO pages (display_order) VALUES (0)", [])
        .expect("Impossibile inserire la prima pagina");
    
    let first_id = conn.last_insert_rowid();
    let initial_bg = backgrounds[0].clone();
    
    update_page_background(&conn, first_id, &initial_bg)
        .expect("Impossibile aggiornare lo sfondo della prima pagina");
        
    conn.execute(
        "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)",
        rusqlite::params![first_id, encode_payload_list(&[])],
    ).expect("Impossibile inserire il base layer");

    let mut expected_backgrounds = vec![initial_bg];

    println!("[TEST] 3. Creazione di 9 pagine aggiuntive...");
    for i in 1..10 {
        let bg = &backgrounds[i % backgrounds.len()];
        expected_backgrounds.push(bg.clone());
        insert_page_after(&conn, i - 1, bg)
            .expect("Impossibile inserire la pagina successiva");
    }

    println!("[TEST] 4. Verifica 1: Controllo numero pagine e sfondi...");
    assert_eq!(page_count(&conn).unwrap(), 10, "Devono esserci esattamente 10 pagine");

    for i in 0..10 {
        let page_id = page_id_at(&conn, i).unwrap();
        let page_data = load_page(&conn, page_id).unwrap();
        assert_eq!(page_data.background, expected_backgrounds[i], "Sfondo errato all'indice {}", i);
    }

    println!("[TEST] 5. Verifica 2: Spostamento pagina...");
    let original_idx = 2; 
    let new_idx_down = 7; 
    let id_to_move = page_id_at(&conn, original_idx).unwrap();
    
    move_page(&conn, id_to_move, new_idx_down).unwrap();
    let id_at_new_idx = page_id_at(&conn, new_idx_down).unwrap();
    assert_eq!(id_to_move, id_at_new_idx, "Spostamento in giù fallito");

    move_page(&conn, id_to_move, original_idx).unwrap();
    let id_at_restored_idx = page_id_at(&conn, original_idx).unwrap();
    assert_eq!(id_to_move, id_at_restored_idx, "Spostamento in su fallito");

    println!("[TEST] 6. Verifica 3: Inserimento nuova pagina (Su/Giù) e thumbnail vuota...");
    let ref_idx = 5; 
    let ref_id = page_id_at(&conn, ref_idx).unwrap();
    let ref_page = load_page(&conn, ref_id).unwrap();

    let new_id_after = insert_page_after(&conn, ref_idx, &ref_page.background).unwrap();
    let new_page_after = load_page(&conn, new_id_after).unwrap();
    assert_eq!(new_page_after.background, ref_page.background);
    assert!(new_page_after.components.is_empty());

    let new_id_before = insert_page_before(&conn, ref_idx, &ref_page.background).unwrap();
    let new_page_before = load_page(&conn, new_id_before).unwrap();
    assert_eq!(new_page_before.background, ref_page.background);
    assert!(new_page_before.components.is_empty());

    assert_eq!(page_count(&conn).unwrap(), 12, "Le pagine totali devono essere 12");

    println!("[TEST] 7. Verifica 4: Eliminazione pagine...");
    let mut expected_ids: Vec<i64> = (0..12).map(|i| page_id_at(&conn, i).unwrap()).collect();
    
    let index_to_delete_1 = 8;
    let index_to_delete_2 = 3;
    
    let id_to_delete_1 = expected_ids[index_to_delete_1];
    let id_to_delete_2 = expected_ids[index_to_delete_2];
    
    delete_page(&conn, id_to_delete_1).unwrap();
    delete_page(&conn, id_to_delete_2).unwrap();
    
    expected_ids.remove(index_to_delete_1);
    expected_ids.remove(index_to_delete_2);
    
    println!("[TEST] 8. Verifica 4: Riapertura documento post-eliminazione...");
    assert_eq!(page_count(&conn).unwrap(), 10, "Le pagine devono essere tornate 10");
    
    for i in 0..10 {
        let loaded_id = page_id_at(&conn, i).unwrap();
        assert_eq!(loaded_id, expected_ids[i], "Ordine corrotto all'indice {}", i);
        assert_ne!(loaded_id, id_to_delete_1, "ID eliminato 1 riapparso");
        assert_ne!(loaded_id, id_to_delete_2, "ID eliminato 2 riapparso");
    }

    println!("[TEST] TEST COMPLETATO CON SUCCESSO!");
}