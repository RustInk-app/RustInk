use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::{color::Color, page::PAGE_H, page::PAGE_W, page::Page, stroke::Stroke};

#[derive(Debug)]
pub enum XoppError {
    Io(std::io::Error),
    Xml(String),
    InvalidData(String),
}

impl std::fmt::Display for XoppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XoppError::Io(e) => write!(f, "I/O: {e}"),
            XoppError::Xml(s) => write!(f, "XML: {s}"),
            XoppError::InvalidData(s) => write!(f, "Dati non validi: {s}"),
        }
    }
}

impl From<std::io::Error> for XoppError {
    fn from(e: std::io::Error) -> Self {
        XoppError::Io(e)
    }
}

pub fn import_xopp(path: &Path) -> Result<Vec<Page>, XoppError> {
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
}

fn parse_xopp_xml(xml: &str) -> Result<Vec<Page>, XoppError> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut pages: Vec<Page> = Vec::new();

    let mut state = ParseState::Root;

    let mut page_w: f64 = PAGE_W;
    let mut page_h: f64 = PAGE_H;

    let mut stroke_color: Color = Color::black();
    let mut stroke_width: f64 = 1.41;
    let mut stroke_text: String = String::new();

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"page" => {
                    if state != ParseState::Root {
                        return Err(XoppError::Xml("<page> annidato inaspettato".into()));
                    }

                    page_w = attr_f64(e, b"width").unwrap_or(PAGE_W);
                    page_h = attr_f64(e, b"height").unwrap_or(PAGE_H);
                    pages.push(Page::new());
                    state = ParseState::InPage;
                }
                b"layer" if state == ParseState::InPage => {
                    state = ParseState::InLayer;
                }
                b"stroke" if state == ParseState::InLayer => {
                    let tool = attr_str(e, b"tool").unwrap_or_default();
                    if tool == "pen" {
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
                _ => {}
            },

            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"stroke" if state == ParseState::InStroke => {
                    if let Some(page) = pages.last_mut() {
                        if let Some(stroke) = build_stroke(
                            &stroke_text,
                            stroke_color.clone(),
                            stroke_width,
                            page_w,
                            page_h,
                        ) {
                            page.strokes.push(stroke);
                        }
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

            Ok(Event::Eof) => break,

            Err(e) => return Err(XoppError::Xml(e.to_string())),

            _ => {}
        }
        buf.clear();
    }

    Ok(pages)
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

fn parse_xopp_color(s: &str) -> Color {
    let s = s.trim_start_matches('#');

    let parse = |start: usize| -> f64 {
        u8::from_str_radix(s.get(start..start + 2).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0
    };
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
