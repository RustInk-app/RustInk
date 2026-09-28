use rustInk::models::page::{PageData, ComponentPayload, ShapeBlock, ShapeKind};
use rustInk::models::stroke::Stroke;
use rustInk::models::image::ImageBlock;
use rustInk::models::textbox::{RichTextBlock, TextSpan, TextStyle};
use rustInk::models::color::Color;
use rustInk::models::select::hit_test_component;


fn setup_test_page() -> PageData {
    let mut page = PageData::new();
    
    page.components.push(ComponentPayload::PenStroke(Stroke {
        points: vec![(50.0, 50.0), (100.0, 100.0)], color: Color::black(), width: 5.0,
    }));
    page.components.push(ComponentPayload::Shape(ShapeBlock {
        kind: ShapeKind::Rectangle, x1: 200.0, y1: 200.0, x2: 250.0, y2: 250.0, color: Color::black(), width: 2.0,
    }));
    page.components.push(ComponentPayload::RichText(RichTextBlock {
        id_temporaneo: "txt_1".to_string(), x: 300.0, y: 300.0, width: 100.0, spans: vec![TextSpan { text: "Test".to_string(), style: TextStyle::default() }],
    }));
    page.components.push(ComponentPayload::Image(ImageBlock {
        filename: "test.webp".to_string(), x: 400.0, y: 400.0, width: 100.0, height: 100.0,
    }));

    page
}

#[test]
fn test_eraser_deletes_pen_strokes() {
    let page = setup_test_page();
    
    let hit = hit_test_component(&page, 75.0, 75.0);
    assert_eq!(hit, Some(0), "Il motore di selezione deve trovare il PenStroke.");
    
    
    let can_delete = matches!(page.components[hit.unwrap()], ComponentPayload::PenStroke(_));
    assert!(can_delete, "ERRORE: L'interfaccia non permetterebbe di cancellare il PenStroke.");
}

#[test]
fn test_eraser_ignores_shapes() {
    let page = setup_test_page();
    
    
    let hit = hit_test_component(&page, 225.0, 225.0);
    assert!(hit.is_some(), "Il motore di selezione deve trovare la forma");
    
    
    let can_delete = matches!(page.components[hit.unwrap()], ComponentPayload::PenStroke(_));
    assert!(!can_delete, "PERICOLO: L'interfaccia crederebbe che la forma sia un PenStroke e la eliminerebbe!");
}

#[test]
fn test_eraser_ignores_text() {
    let page = setup_test_page();
    
    let hit = hit_test_component(&page, 310.0, 310.0);
    assert!(hit.is_some(), "Il motore di selezione deve trovare il testo");
    
    let can_delete = matches!(page.components[hit.unwrap()], ComponentPayload::PenStroke(_));
    assert!(!can_delete, "PERICOLO: L'interfaccia eliminerebbe il testo!");
}

#[test]
fn test_eraser_ignores_images() {
    let page = setup_test_page();
    
    let hit = hit_test_component(&page, 450.0, 450.0);
    assert!(hit.is_some(), "Il motore di selezione deve trovare l'immagine");
    
    let can_delete = matches!(page.components[hit.unwrap()], ComponentPayload::PenStroke(_));
    assert!(!can_delete, "PERICOLO: L'interfaccia eliminerebbe l'immagine!");
}