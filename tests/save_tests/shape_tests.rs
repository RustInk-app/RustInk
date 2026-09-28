use rustInk::models::page::{ComponentPayload, ShapeBlock, ShapeKind};
use rustInk::models::color::Color;
use rustInk::save_handler::db::{init_schema, load_page, encode_payload_list};
use rustInk::save_handler::database_utilities::append_active_component;
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
fn test_all_shapes_roundtrip() {
    let (conn, page_id) = setup_memory_db();

    
    let shapes_to_test = vec![
        (ShapeKind::Rectangle, "Rettangolo"),
        (ShapeKind::Ellipse, "Ellisse"),
        (ShapeKind::Arrow, "Freccia"),
        (ShapeKind::DoubleArrow, "Doppia Freccia"),
        (ShapeKind::Line, "Linea Retta"),
    ];

    
    for (i, (kind, name)) in shapes_to_test.into_iter().enumerate() {
        let offset = (i as f64) * 50.0;
        let shape_block = ShapeBlock {
            kind,
            x1: 10.0 + offset,
            y1: 20.0 + offset,
            x2: 110.0 + offset,
            y2: 120.0 + offset,
            color: Color::new(0.1, 0.2, 0.3), 
            width: 3.5,                       
        };

        let payload = ComponentPayload::Shape(shape_block);
        let result = append_active_component(&conn, page_id, &payload);
        assert!(result.is_ok(), "Fallimento nel salvataggio della shape: {}", name);
    }

    
    let page = load_page(&conn, page_id).expect("Errore nel caricamento della pagina con le shape");

    
    assert_eq!(page.components.len(), 5, "Non tutte le shape sono state salvate nel DB.");

    
    let expected_kinds = vec![
        ShapeKind::Rectangle,
        ShapeKind::Ellipse,
        ShapeKind::Arrow,
        ShapeKind::DoubleArrow,
        ShapeKind::Line,
    ];

    for (i, expected_kind) in expected_kinds.into_iter().enumerate() {
        if let ComponentPayload::Shape(saved_shape) = &page.components[i] {
            assert_eq!(saved_shape.kind, expected_kind, "Il tipo di shape non corrisponde all'indice {}", i);
            assert_eq!(saved_shape.width, 3.5, "Lo spessore della shape {} è stato alterato", i);
            assert_eq!(saved_shape.color.r, 0.1, "Il canale colore rosso della shape {} è errato", i);
            assert!(saved_shape.x2 > saved_shape.x1, "Le coordinate x della shape {} sono corrotte", i);
        } else {
            panic!("Il componente all'indice {} non è una Shape!", i);
        }
    }
}