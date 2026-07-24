use RASTIN::models::color::Color; //[cite: 1]
use RASTIN::models::stroke::Stroke; //[cite: 1]
use RASTIN::models::page::ComponentPayload; //[cite: 1]
use RASTIN::save_handler::database_utilities::append_active_component; //[cite: 1]
use RASTIN::save_handler::db::{init_schema, load_page, export_medias, import_medias}; //[cite: 1]

use rusqlite::Connection;
use std::path::Path;
use std::fs;
use tempfile::tempdir;

// =========================================================================
// TEST 1: Funzionalità Base 
// (Verifica che due tratti standard vengano salvati e recuperati correttamente)
// =========================================================================
#[test]
fn test_penstroke_basic_functionality() {
    let dir = tempdir().expect("Impossibile creare la cartella temporanea");
    
    let db_tmp_path = dir.path().join("struttura_base.sqlite");
    let rastin_path = dir.path().join("documento_base.rastin");
    let db_recovery_path = dir.path().join("recovery_base.sqlite");

    let stroke1 = Stroke {
        points: vec![(10.0, 50.0), (20.0, 60.0), (40.0, 20.0)],
        color: Color::new(1.0, 0.0, 0.0), //[cite: 1]
        width: 3.0,
    };
    let payload1 = ComponentPayload::PenStroke(stroke1.clone()); //[cite: 1]

    let stroke2 = Stroke {
        points: vec![(100.0, 100.0), (200.0, 100.0)],
        color: Color::new(0.0, 0.0, 1.0), //[cite: 1]
        width: 6.0,
    };
    let payload2 = ComponentPayload::PenStroke(stroke2.clone()); //[cite: 1]

    {
        let conn = Connection::open(&db_tmp_path).expect("Apertura DB");
        init_schema(&conn).expect("Init schema"); //[cite: 1]
        
        conn.execute("INSERT INTO pages (display_order) VALUES (0)", []).unwrap();
        let page_id = conn.last_insert_rowid();

        append_active_component(&conn, page_id, &payload1).unwrap(); //[cite: 1]
        append_active_component(&conn, page_id, &payload2).unwrap(); //[cite: 1]

        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }

    export_medias(&db_tmp_path, &rastin_path).expect("Esportazione fallita"); //[cite: 1]

    {
        import_medias(&rastin_path, &db_recovery_path).expect("Importazione fallita"); //[cite: 1]
        
        let conn = Connection::open(&db_recovery_path).expect("Apertura DB recuperato");
        let page_data = load_page(&conn, 1).expect("Caricamento pagina"); //[cite: 1]

        assert_eq!(page_data.components.len(), 2, "La pagina deve contenere 2 componenti");

        match &page_data.components[0] {
            ComponentPayload::PenStroke(recovered1) => {
                assert_eq!(recovered1.points, stroke1.points);
                assert_eq!(recovered1.color, stroke1.color);
                assert_eq!(recovered1.width, stroke1.width);
            }
            _ => panic!("Il primo componente non è un PenStroke"),
        }

        match &page_data.components[1] {
            ComponentPayload::PenStroke(recovered2) => {
                assert_eq!(recovered2.points, stroke2.points);
                assert_eq!(recovered2.color, stroke2.color);
                assert_eq!(recovered2.width, stroke2.width);
            }
            _ => panic!("Il secondo componente non è un PenStroke"),
        }
    }
}

// =========================================================================
// TEST 2: Salvataggio e recupero multiplo 
// (Crea 20 file .rastin ispezionabili in una cartella per verificare l'integrità dinamica)
// =========================================================================
#[test]
fn test_salvataggio_e_recupero_penstroke_multiplo() {
    let docs_da_creare = 20;

    let output_dir = Path::new("./test_output");
    fs::create_dir_all(output_dir).expect("Impossibile creare test_output");

    for i in 0..docs_da_creare {
        let db_tmp_path = output_dir.join(format!("tmp_struttura_{i}.sqlite"));
        let rastin_path = output_dir.join(format!("documento_{i}.rastin"));

        let width = 1.5 + (i as f64 * 0.5); 
        let r = (i as f64 * 10.0 % 255.0) / 255.0;
        let g = (i as f64 * 20.0 % 255.0) / 255.0;
        let b = (i as f64 * 30.0 % 255.0) / 255.0;
        let color = Color::new(r, g, b); //[cite: 1]

        let points = vec![(10.0, 10.0), (20.0, i as f64 * 5.0), (30.0, 30.0)];

        let original_stroke = Stroke { points: points.clone(), color: color.clone(), width };
        let payload = ComponentPayload::PenStroke(original_stroke); //[cite: 1]

        {
            let conn = Connection::open(&db_tmp_path).expect("Apertura DB");
            init_schema(&conn).expect("Inizializzazione schema"); //[cite: 1]

            conn.execute("INSERT INTO pages (display_order) VALUES (0)", []).unwrap();
            let page_id = conn.last_insert_rowid();

            append_active_component(&conn, page_id, &payload).expect("Salvataggio fallito"); //[cite: 1]
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        } 
        
        export_medias(&db_tmp_path, &rastin_path).expect("Export fallito"); //[cite: 1]

        {
            let db_recovery_path = output_dir.join(format!("recovery_{i}.sqlite"));
            import_medias(&rastin_path, &db_recovery_path).expect("Import fallita"); //[cite: 1]
            
            let conn = Connection::open(&db_recovery_path).expect("Riapertura DB");
            let page_data = load_page(&conn, 1).expect("Caricamento pagina"); //[cite: 1]

            assert_eq!(page_data.components.len(), 1, "Il documento deve avere 1 componente");

            match &page_data.components[0] {
                ComponentPayload::PenStroke(recovered_stroke) => {
                    assert_eq!(recovered_stroke.width, width);
                    assert_eq!(recovered_stroke.color, color);
                    assert_eq!(recovered_stroke.points, points);
                }
                _ => panic!("Il componente non è un PenStroke!"),
            }
            
            let _ = fs::remove_file(db_tmp_path);
            let _ = fs::remove_file(db_recovery_path);
        }
    }
}

// =========================================================================
// TEST 3: Iniezione di Caos Totale
// (Garantisce che il Database respinga NaN, Infinity e valori oltre f32::MAX)
// =========================================================================
#[test]
fn test_penstroke_total_chaos_injection() {
    let output_dir = Path::new("./test_output_chaos");
    fs::create_dir_all(output_dir).expect("Impossibile creare test_output_chaos");

    let chaos_cases = vec![
        (Stroke { points: vec![(f64::INFINITY, f64::NEG_INFINITY)], color: Color::black(), width: 2.0 }, true),
        (Stroke { points: vec![(f64::NAN, f64::NAN)], color: Color::black(), width: 2.0 }, true),
        (Stroke { points: vec![(f64::MAX, f64::MAX)], color: Color::black(), width: 2.0 }, true),
        (Stroke { points: vec![(50.0, 50.0), (60.0, 60.0)], color: Color::black(), width: -10.0 }, false),
        (Stroke { points: vec![(10.0, 10.0), (20.0, 20.0)], color: Color::new(-50.0, 999.0, f64::NAN), width: 2.0 }, false),
        (Stroke { points: vec![], color: Color::black(), width: 2.0 }, false),
    ];

    for (i, (malicious_stroke, should_be_blocked)) in chaos_cases.into_iter().enumerate() {
        let db_tmp_path = output_dir.join(format!("chaos_{i}.sqlite"));
        let payload = ComponentPayload::PenStroke(malicious_stroke); //[cite: 1]

        let conn = Connection::open(&db_tmp_path).expect("Apertura DB");
        init_schema(&conn).expect("Init schema fallito"); //[cite: 1]

        conn.execute("INSERT INTO pages (display_order) VALUES (0)", []).unwrap();
        let page_id = conn.last_insert_rowid();

        let result = append_active_component(&conn, page_id, &payload); //[cite: 1]

        if should_be_blocked {
            assert!(result.is_err(), "PERICOLO! Caso Chaos {i} accettato. Il filtro ha fallito.");
        } else {
            assert!(result.is_ok(), "ERRORE! Caso Chaos {i} respinto ma doveva passare.");
        }
        
        let _ = fs::remove_file(db_tmp_path);
    }
}