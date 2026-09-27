use RASTIN::models::page::ComponentPayload;
use RASTIN::models::textbox::{RichTextBlock, TextSpan, TextStyle};
use RASTIN::models::color::Color;
use RASTIN::save_handler::db::{init_schema, load_page, encode_payload_list};
use RASTIN::save_handler::database_utilities::append_active_component;
use rusqlite::Connection;


fn setup_memory_db() -> (Connection, i64) {
    let conn = Connection::open_in_memory().expect("Impossibile aprire DB in memoria");
    init_schema(&conn).expect("Init schema fallito"); 
    
    
    conn.execute("INSERT INTO pages (display_order) VALUES (0)", []).unwrap(); 
    let page_id = conn.last_insert_rowid();
    
    
    let empty_blob = encode_payload_list(&[]); 
    conn.execute(
        "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)", 
        rusqlite::params![page_id, empty_blob]
    ).unwrap(); 

    (conn, page_id)
}

#[test]
fn test_textbox_basic_properties_roundtrip() {
    let (conn, page_id) = setup_memory_db();

    
    let original_text = "Testo con a capo\nE un ritorno a carrello\r\nE caratteri speciali: 🚀 @#!!";
    let original_style = TextStyle {
        font_family: "Comic Sans MS".to_string(), 
        size: 42.0,                               
        color: Color::new(1.0, 0.0, 0.5),         
        bold: true,                               
        italic: false,
    };

    let payload = ComponentPayload::RichText(RichTextBlock {
        id_temporaneo: "txt_test_1".to_string(),
        x: 100.0,
        y: 150.0,
        width: 300.0,
        spans: vec![TextSpan {
            text: original_text.to_string(),
            style: original_style.clone(),
        }],
    }); 

    
    let result = append_active_component(&conn, page_id, &payload); 
    assert!(result.is_ok(), "Fallimento nel salvataggio del blocco di testo nel DB.");

    
    let page = load_page(&conn, page_id).expect("Errore nel caricamento della pagina"); 
    
    
    assert_eq!(page.components.len(), 1, "Il componente testo non è stato salvato correttamente.");

    if let ComponentPayload::RichText(saved_block) = &page.components[0] {
        assert_eq!(saved_block.x, 100.0);
        assert_eq!(saved_block.y, 150.0);
        assert_eq!(saved_block.width, 300.0);
        assert_eq!(saved_block.spans.len(), 1);
        
        let saved_span = &saved_block.spans[0];
        assert_eq!(saved_span.text, original_text, "I caratteri speciali (\\n, \\r, emoji) sono stati corrotti!");
        
        
        assert_eq!(saved_span.style.font_family, "Comic Sans MS", "Font perso durante il salvataggio");
        assert_eq!(saved_span.style.size, 42.0, "Size persa durante il salvataggio");
        assert!(saved_span.style.bold, "Grassetto perso durante il salvataggio");
        assert_eq!(saved_span.style.color.r, 1.0, "Colore rosso alterato");
        assert_eq!(saved_span.style.color.b, 0.5, "Colore blu alterato");
    } else {
        panic!("Il payload caricato non è un RichText!");
    }
}

#[test]
fn test_multiple_textboxes_and_volume_bbox() {
    let (conn, page_id) = setup_memory_db();

    
    let short_block = RichTextBlock {
        id_temporaneo: "txt_short".to_string(),
        x: 50.0, y: 50.0, width: 800.0, 
        spans: vec![TextSpan {
            text: "Titolo Breve".to_string(), 
            style: TextStyle { size: 12.0, ..Default::default() }
        }],
    }; 

    
    let huge_text = "Parola ".repeat(5000); 
    let tall_block = RichTextBlock {
        id_temporaneo: "txt_tall".to_string(),
        x: 100.0, y: 100.0, width: 50.0, 
        spans: vec![TextSpan {
            text: huge_text,
            style: TextStyle { size: 10.0, ..Default::default() }
        }],
    }; 

    
    let multispan_block = RichTextBlock {
        id_temporaneo: "txt_multi".to_string(),
        x: 200.0, y: 200.0, width: 300.0,
        spans: vec![
            TextSpan { text: "Inizio ".to_string(), style: TextStyle::default() },
            TextSpan { text: "Rosso ".to_string(), style: TextStyle { color: Color::new(1.0,0.0,0.0), ..Default::default() } },
            TextSpan { text: "Fine".to_string(), style: TextStyle::default() },
        ],
    }; 

    
    append_active_component(&conn, page_id, &ComponentPayload::RichText(short_block.clone())).unwrap(); 
    append_active_component(&conn, page_id, &ComponentPayload::RichText(tall_block.clone())).unwrap(); 
    append_active_component(&conn, page_id, &ComponentPayload::RichText(multispan_block.clone())).unwrap(); 

    
    let page = load_page(&conn, page_id).unwrap(); 
    assert_eq!(page.components.len(), 3, "Non tutti i blocchi di testo sono stati caricati!");

    
    
    
    
    let (_, _, _, short_h) = short_block.approx_bbox(); 
    let line_height = 12.0 * 1.6; 
    assert_eq!(short_h - short_block.y, line_height, "Il blocco corto dovrebbe occupare esattamente una riga in altezza.");

    
    
    let (_, _, _, tall_h) = tall_block.approx_bbox(); 
    let calculated_height = tall_h - tall_block.y;
    assert!(
        calculated_height > 1000.0, 
        "La box con 35.000 caratteri in width=50 non si è auto-ridimensionata! Altezza calcolata: {}", 
        calculated_height
    );

    
    if let ComponentPayload::RichText(loaded_multi) = &page.components[2] {
        assert_eq!(loaded_multi.spans.len(), 3, "Gli span multipli sono andati persi");
        assert_eq!(loaded_multi.spans[1].style.color.r, 1.0, "Il colore dello span centrale è corretto");
    }
}

#[test]
fn test_multiple_texts_with_zoom_fluctuations_on_same_page() {
    let (conn, page_id) = setup_memory_db();
    
    
    let mut current_page = load_page(&conn, page_id).unwrap(); 
    let mut simulated_zoom: f64;

    
    
    
    let short_text = RichTextBlock {
        id_temporaneo: "txt_short".to_string(),
        x: 50.0, y: 50.0, width: 200.0,
        spans: vec![TextSpan { 
            text: "Ciao RASTIN!".to_string(), 
            style: TextStyle::default() 
        }],
    }; 
    
    
    let payload1 = ComponentPayload::RichText(short_text); 
    append_active_component(&conn, page_id, &payload1).unwrap(); 
    current_page.components.push(payload1); 

    
    for i in 1..=10 {
        simulated_zoom = 0.5 + (i as f64 * 0.2); 
        assert_eq!(current_page.components.len(), 1, "Il numero di componenti deve essere 1");
    }

    
    
    
    let medium_text = RichTextBlock {
        id_temporaneo: "txt_med".to_string(),
        x: 100.0, y: 150.0, width: 350.0,
        spans: vec![TextSpan { 
            text: "Questo è un testo di media lunghezza. Serve a testare se l'aggiunta \
                   di un secondo componente interferisce con il primo durante continui \
                   ridimensionamenti del canvas.".to_string(), 
            style: TextStyle { size: 14.0, ..Default::default() } 
        }],
    }; 

    let payload2 = ComponentPayload::RichText(medium_text); 
    append_active_component(&conn, page_id, &payload2).unwrap(); 
    current_page.components.push(payload2); 

    
    for i in 1..=10 {
        simulated_zoom = 3.0 - (i as f64 * 0.15);
        assert_eq!(current_page.components.len(), 2, "Il numero di componenti deve essere 2");
    }

    
    
    
    let lorem_ipsum = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
        quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. \
        Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat \
        nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui \
        officia deserunt mollit anim id est laborum.\n\n".repeat(10); 

    let long_text = RichTextBlock {
        id_temporaneo: "txt_long".to_string(),
        x: 20.0, y: 300.0, width: 500.0,
        spans: vec![TextSpan { 
            text: lorem_ipsum, 
            style: TextStyle { size: 10.0, color: Color::new(0.2, 0.2, 0.2), ..Default::default() } 
        }],
    }; 

    let payload3 = ComponentPayload::RichText(long_text); 
    append_active_component(&conn, page_id, &payload3).unwrap(); 
    current_page.components.push(payload3); 

    
    for i in 1..=10 {
        simulated_zoom = (100 * i/2) as f64;
        
        
        assert_eq!(current_page.components.len(), 3, "Il numero di componenti deve essere 3");

        
        for comp in &current_page.components { 
            if let ComponentPayload::RichText(block) = comp {
                let visual_width = block.width * simulated_zoom;
                let (_, _, _, approx_h) = block.approx_bbox(); 
                let visual_height = (approx_h - block.y) * simulated_zoom;
                
                assert!(visual_width > 0.0 && visual_height > 0.0, "La proiezione visiva ha fallito!");
            }
        }
    }
    
    
    
    let saved_page = load_page(&conn, page_id).unwrap(); 
    assert_eq!(saved_page.components.len(), 3, "Il database non ha salvato tutti e 3 i testi sulla pagina.");
}