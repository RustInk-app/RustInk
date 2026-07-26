#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use RASTIN::models::stroke::Stroke;
use RASTIN::models::color::Color;
use RASTIN::models::page::ComponentPayload;
use RASTIN::save_handler::db::*;

// Creiamo una struttura derivata da Arbitrary per istruire il fuzzer 
// su come generare i dati per il nostro Stroke.
#[derive(Arbitrary, Debug)]
struct FuzzStrokeInput {
    // Fuzzerà tuple di coordinate (f64, f64) di lunghezza casuale
    points: Vec<(f64, f64)>, 
    // Fuzzerà i canali colore
    r: f64,
    g: f64,
    b: f64,
    // Fuzzerà lo spessore del tratto
    width: f64,
}

fuzz_target!(|data: FuzzStrokeInput| {
    // 1. Ricostruiamo la tua struttura dati reale a partire dai dati caotici
    let color = Color::new(data.r, data.g, data.b);
    
    let stroke = Stroke {
        points: data.points,
        color,
        width: data.width,
    };

    let payload = ComponentPayload::PenStroke(stroke);

    // 2. STRESS TEST MATEMATICO
    // Passiamo il tratto generato casualmente alla tua logica del Bounding Box.
    // Qui il fuzzer cercherà di iniettare NaN, Infinity o array di punti enormi
    // per vedere se il ciclo for e le funzioni .min() / .max() resistono ai crash.
    let (min_x, max_x, min_y, max_y) = bounding_box(&payload);

    // 3. (Opzionale) STRESS TEST DI SERIALIZZAZIONE
    // Verifica che bincode riesca a serializzare/deserializzare questo tratto 
    // caotico senza andare in panic (come avviene in encode_payload)
    let serialized = encode_payload(&payload);
    let _deserialized = decode_payload(&serialized);
});