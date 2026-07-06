// translate_xournal.rs
use std::io::Read;
use std::path::Path;

use base64::Engine;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::color::Color;
use crate::page::{ComponentPayload, PaperBackground, PAGE_H, PAGE_W};
use crate::textbox::{RichTextBlock, TextSpan, TextStyle};
use crate::image::ImageBlock;
use crate::save_handler::autosave;
use crate::stroke::Stroke;

#[derive(Debug)]
pub enum XoppError {
    Io(std::io::Error),
    Xml(String),
    Image(String),
}

impl std::fmt::Display for XoppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XoppError::Io(e) => write!(f, "I/O: {e}"),
            XoppError::Xml(s) => write!(f, "XML: {s}"),
            XoppError::Image(s) => write!(f, "Immagine: {s}"),
        }
    }
}

impl From<std::io::Error> for XoppError {
    fn from(e: std::io::Error) -> Self {
        XoppError::Io(e)
    }
}

/// Una pagina importata da un file .xopp, già nel linguaggio del nuovo
/// modello RASTIN a componenti (pronta per essere scritta in `pages` +
/// `base_layers` senza ulteriori conversioni).
pub struct XoppPage {
    pub background: PaperBackground,
    pub components: Vec<ComponentPayload>,
}

pub fn import_xopp(path: &Path) -> Result<Vec<XoppPage>, XoppError> {
    let file = std::fs::File::open(path)?;
    let mut gz = flate2::read::GzDecoder::new(file);
    let mut xml = String::new();
    gz.read_to_string(&mut xml)?;

    parse_xopp_xml(&xml)
}

#[derive(PartialEq)]
enum ParseState {
    Root,
    InPage,
    InLayer,
    InStroke,
    InText,
    InImage,
}

fn parse_xopp_xml(xml: &str) -> Result<Vec<XoppPage>, XoppError> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut pages: Vec<XoppPage> = Vec::new();
    let mut state = ParseState::Root;

    // Dimensioni della pagina xopp corrente, per calcolare il fattore di scala
    // verso il formato canonico RASTIN (PAGE_W x PAGE_H).
    let mut src_w: f64 = PAGE_W;
    let mut src_h: f64 = PAGE_H;

    // --- Stato temporaneo per <stroke> ---
    let mut stroke_color = Color::black();
    let mut stroke_width: f64 = 1.41;
    let mut stroke_text = String::new();

    // --- Stato temporaneo per <text> ---
    let mut text_x: f64 = 0.0;
    let mut text_y: f64 = 0.0;
    let mut text_size: f64 = 12.0;
    let mut text_color = Color::black();
    let mut text_content = String::new();

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"page" => {
                    if state != ParseState::Root {
                        return Err(XoppError::Xml("<page> annidato inaspettato".into()));
                    }
                    src_w = attr_f64(e, b"width").unwrap_or(PAGE_W);
                    src_h = attr_f64(e, b"height").unwrap_or(PAGE_H);
                    pages.push(XoppPage {
                        background: PaperBackground::Ruled,
                        components: Vec::new(),
                    });
                    state = ParseState::InPage;
                }
                b"layer" if state == ParseState::InPage => {
                    state = ParseState::InLayer;
                }
                b"stroke" if state == ParseState::InLayer => {
                    let tool = attr_str(e, b"tool").unwrap_or_default();
                    // "highlighter" viene trattato come penna: non esiste ancora
                    // un ComponentPayload dedicato per l'evidenziatore.
                    if tool == "pen" || tool == "highlighter" {
                        stroke_color = attr_str(e, b"color")
                            .map(|s| parse_xopp_color(&s))
                            .unwrap_or_else(Color::black);
                        stroke_width = attr_str(e, b"width")
                            .map(|s| parse_width_list(&s))
                            .unwrap_or(1.41);
                        stroke_text.clear();
                        state = ParseState::InStroke;
                    }
                }
                b"text" if state == ParseState::InLayer => {
                    text_x = attr_f64(e, b"x").unwrap_or(0.0);
                    text_y = attr_f64(e, b"y").unwrap_or(0.0);
                    text_size = attr_f64(e, b"size").unwrap_or(12.0);
                    text_color = attr_str(e, b"color")
                        .map(|s| parse_xopp_color(&s))
                        .unwrap_or_else(Color::black);
                    text_content.clear();
                    state = ParseState::InText;
                }
                b"image" if state == ParseState::InLayer => {
                    // Il contenuto base64 arriva come testo dentro l'elemento,
                    // ma lo gestiamo interamente qui sotto in Event::End per
                    // avere già raccolto left/top/right/bottom.
                    text_x = attr_f64(e, b"left").unwrap_or(0.0);
                    text_y = attr_f64(e, b"top").unwrap_or(0.0);
                    // Riuso text_size/text_color come scratch per right/bottom
                    // sarebbe fuorviante: uso variabili dedicate qui sotto.
                    state = ParseState::InImage;
                    IMG_RIGHT.with(|c| *c.borrow_mut() = attr_f64(e, b"right").unwrap_or(text_x));
                    IMG_BOTTOM.with(|c| *c.borrow_mut() = attr_f64(e, b"bottom").unwrap_or(text_y));
                    text_content.clear();
                }
                _ => {}
            },

            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"background" && state == ParseState::InPage => {
                let style = attr_str(e, b"style").unwrap_or_default();
                if let Some(page) = pages.last_mut() {
                    page.background = map_background_style(&style);
                }
            }

            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"stroke" if state == ParseState::InStroke => {
                    if let Some(page) = pages.last_mut() {
                        if let Some(stroke) = build_stroke(
                            &stroke_text,
                            stroke_color.clone(),
                            stroke_width,
                            src_w,
                            src_h,
                        ) {
                            page.components.push(ComponentPayload::PenStroke(stroke));
                        }
                    }
                    state = ParseState::InLayer;
                }
                b"text" if state == ParseState::InText => {
                    if !text_content.trim().is_empty() {
                        if let Some(page) = pages.last_mut() {
                            let sx = PAGE_W / src_w;
                            let sy = PAGE_H / src_h;
                            let scale = (sx + sy) / 2.0;

                            let text_font: String = "Sans".to_string();

                            let font_size_scaled = text_size * scale;

                            // Xournal++ non salva una larghezza per il blocco di testo: la stimiamo
                            // dalla riga più lunga (in caratteri) moltiplicata per una larghezza
                            // media di carattere approssimata come 0.55 * font_size.
                            let longest_line = text_content
                                .lines()
                                .map(|l| l.chars().count())
                                .max()
                                .unwrap_or(0);
                            let estimated_width = (longest_line as f64 * font_size_scaled * 0.55).max(20.0);

                            page.components.push(ComponentPayload::RichText(RichTextBlock {
                                id_temporaneo: uuid::Uuid::new_v4().to_string(),
                                x: text_x * sx,
                                y: text_y * sy,
                                width: estimated_width,
                                spans: vec![TextSpan {
                                    text: text_content.clone(),
                                    style: TextStyle {
                                        // xopp usa sempre font="Sans": non abbiamo un vero nome
                                        // font da mappare, quindi teniamo il valore letto dall'attributo.
                                        font_family: text_font.clone(),
                                        size: font_size_scaled,
                                        color: text_color.clone(),
                                        // Xournal++ non distingue bold/italic nell'elemento <text>:
                                        // lo stile è sempre "regolare".
                                        bold: false,
                                        italic: false,
                                    },
                                }],
                            }));
                        }
                    }
                    state = ParseState::InLayer;
                }
                b"image" if state == ParseState::InImage => {
                    let right = IMG_RIGHT.with(|c| *c.borrow());
                    let bottom = IMG_BOTTOM.with(|c| *c.borrow());
                    let left = text_x;
                    let top = text_y;

                    match decode_and_save_image(&text_content, left, top, right, bottom, src_w, src_h) {
                        Ok(payload) => {
                            if let Some(page) = pages.last_mut() {
                                page.components.push(payload);
                            }
                        }
                        Err(e) => eprintln!("[XOPP] Immagine ignorata: {e}"),
                    }
                    state = ParseState::InLayer;
                }
                b"layer" if state == ParseState::InLayer => {
                    state = ParseState::InPage;
                }
                b"page" if state == ParseState::InPage => {
                    state = ParseState::Root;
                }
                _ => {}
            },

            Ok(Event::Text(ref e)) if state == ParseState::InStroke => {
                if let Ok(t) = e.unescape() {
                    stroke_text.push_str(&t);
                }
            }
            Ok(Event::Text(ref e)) if state == ParseState::InText => {
                if let Ok(t) = e.unescape() {
                    text_content.push_str(&t);
                }
            }
            Ok(Event::Text(ref e)) if state == ParseState::InImage => {
                if let Ok(t) = e.unescape() {
                    text_content.push_str(&t);
                }
            }

            Ok(Event::Eof) => break,

            Err(e) => return Err(XoppError::Xml(e.to_string())),

            _ => {}
        }
        buf.clear();
    }

    Ok(pages)
}

// Scratch thread-local per left/top/right/bottom durante il parsing di <image>,
// senza dover complicare la firma degli stati con altri campi dedicati.
thread_local! {
    static IMG_RIGHT: std::cell::RefCell<f64> = std::cell::RefCell::new(0.0);
    static IMG_BOTTOM: std::cell::RefCell<f64> = std::cell::RefCell::new(0.0);
}

fn attr_str(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.to_vec()).ok())
}

fn attr_f64(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<f64> {
    attr_str(e, name)?.parse::<f64>().ok()
}

fn map_background_style(style: &str) -> PaperBackground {
    match style {
        "graph" => PaperBackground::Grid,
        "plain" => PaperBackground::Plain,
        // "lined" / "ruled" / qualunque altro valore
        _ => PaperBackground::Ruled,
    }
}

fn parse_xopp_color(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    let parse = |start: usize| -> f64 {
        u8::from_str_radix(s.get(start..start + 2).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0
    };
    // Il formato xopp è #RRGGBBAA: ignoriamo l'alpha, come già faceva il codice originale.
    Color::new(parse(0), parse(2), parse(4))
}

fn parse_width_list(s: &str) -> f64 {
    let vals: Vec<f64> = s
        .split_whitespace()
        .filter_map(|t| t.parse::<f64>().ok())
        .collect();
    if vals.is_empty() {
        return 1.41;
    }
    vals.iter().sum::<f64>() / vals.len() as f64
}

fn build_stroke(text: &str, color: Color, width: f64, src_w: f64, src_h: f64) -> Option<Stroke> {
    let nums: Vec<f64> = text
        .split_whitespace()
        .filter_map(|t| t.parse::<f64>().ok())
        .collect();

    if nums.len() < 4 || nums.len() % 2 != 0 {
        return None;
    }

    let sx = PAGE_W / src_w;
    let sy = PAGE_H / src_h;

    let points: Vec<(f64, f64)> = nums.chunks(2).map(|c| (c[0] * sx, c[1] * sy)).collect();

    Some(Stroke {
        points,
        color,
        width,
    })
}

/// Decodifica l'immagine base64 inline di un elemento <image>, la ri-codifica
/// in webp (per restare coerenti con la convenzione già usata da import_bundle
/// / export_bundle_path, che filtrano solo file "*.webp" in media/) e la salva
/// nella cartella media della sessione corrente.
fn decode_and_save_image(
    base64_data: &str,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    src_w: f64,
    src_h: f64,
) -> Result<ComponentPayload, XoppError> {
    let clean: String = base64_data.chars().filter(|c| !c.is_whitespace()).collect();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(clean)
        .map_err(|e| XoppError::Image(format!("base64 non valido: {e}")))?;

    let decoded = image::load_from_memory(&raw)
        .map_err(|e| XoppError::Image(format!("formato immagine non supportato: {e}")))?;

    let media_dir = autosave::media_dir();
    std::fs::create_dir_all(&media_dir).map_err(XoppError::Io)?;

    let fname = format!("{}.webp", uuid::Uuid::new_v4());
    let dest = media_dir.join(&fname);

    let mut out = std::fs::File::create(&dest).map_err(XoppError::Io)?;
    // Nota: l'encoder WebP di `image` è lossless; se il progetto usa altrove
    // un encoder lossy (es. crate `webp` con libwebp), sostituire qui per coerenza.
    decoded
        .write_with_encoder(image::codecs::webp::WebPEncoder::new_lossless(&mut out))
        .map_err(|e| XoppError::Image(format!("encoding webp fallito: {e}")))?;

    let sx = PAGE_W / src_w;
    let sy = PAGE_H / src_h;

    Ok(ComponentPayload::Image(ImageBlock {
        filename: format!("media/{fname}"),
        x: left * sx,
        y: top * sy,
        width: (right - left) * sx,
        height: (bottom - top) * sy,
    }))
}