#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use RASTIN::models::stroke::Stroke;
use RASTIN::models::color::Color;
use RASTIN::models::page::ComponentPayload;
use RASTIN::save_handler::db::*;



#[derive(Arbitrary, Debug)]
struct FuzzStrokeInput {
    
    points: Vec<(f64, f64)>, 
    
    r: f64,
    g: f64,
    b: f64,
    
    width: f64,
}

fuzz_target!(|data: FuzzStrokeInput| {
    
    let color = Color::new(data.r, data.g, data.b);
    
    let stroke = Stroke {
        points: data.points,
        color,
        width: data.width,
    };

    let payload = ComponentPayload::PenStroke(stroke);

    
    
    
    
    let (min_x, max_x, min_y, max_y) = bounding_box(&payload);

    
    
    
    let serialized = encode_payload(&payload);
    let _deserialized = decode_payload(&serialized);
});