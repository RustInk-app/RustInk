use crate::models::image;
use crate::models::stroke;
use crate::models::textbox;
use crate::models::color;

use gtk::cairo;

use serde::{Serialize, Deserialize};

pub const PAGE_W: f64 = (21.0 / 2.54) * 72.0; 
pub const PAGE_H: f64 = (29.7 / 2.54) * 72.0; 
pub const PAGE_MARGIN: f64 = 40.0;
pub const LINE_SPACING: f64 = 24.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Tool {
    Pen,
    Eraser,
    Text,
    Select,
    Shape(ShapeKind),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Arrow,
    DoubleArrow,
    Line,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ComponentPayload {
    PenStroke(stroke::Stroke),
    EraserStroke(stroke::Stroke),
    RichText(textbox::RichTextBlock),
    Image(image::ImageBlock),
    Shape(ShapeBlock),
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    
    pub strokes: Vec<stroke::Stroke>,
}

impl Page {
    pub fn new() -> Self {
        Self {
            strokes: Vec::new()
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum PaperBackground {
    Ruled,  
    Plain,  
    Grid,   
}

impl Default for PaperBackground {
    fn default() -> Self { PaperBackground::Grid }
}


#[derive(Clone, Debug, Default)]
pub struct PageData {
    pub components: Vec<ComponentPayload>,
    pub background: PaperBackground,
    pub is_bookmarked: bool,
    pub bookmark_name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfPageRef {
    pub doc_id: i64,
    pub page_index: i64, 
}

impl PageData {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
            background: PaperBackground::Grid, 
            is_bookmarked: false,
            bookmark_name: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShapeBlock {
    pub kind:  ShapeKind,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub color: color::Color,
    pub width: f64,
}

fn draw_arrow_head(cr: &cairo::Context, tip_x: f64, tip_y: f64, from_x: f64, from_y: f64, size: f64) {
    let angle = (tip_y - from_y).atan2(tip_x - from_x);
    let spread = std::f64::consts::PI / 7.0;
    cr.move_to(tip_x, tip_y);
    cr.line_to(tip_x - size * (angle - spread).cos(), tip_y - size * (angle - spread).sin());
    cr.move_to(tip_x, tip_y);
    cr.line_to(tip_x - size * (angle + spread).cos(), tip_y - size * (angle + spread).sin());
    let _ = cr.stroke();
}

pub fn render_shape(cr: &cairo::Context, block: &ShapeBlock, ox: f64, oy: f64) {
    cr.save().ok();
    cr.set_source_rgb(block.color.r, block.color.g, block.color.b);
    cr.set_line_width(block.width);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);

    let (x1, y1, x2, y2) = (ox + block.x1, oy + block.y1, ox + block.x2, oy + block.y2);

    match block.kind {
        ShapeKind::Line => {
            cr.move_to(x1, y1);
            cr.line_to(x2, y2);
            let _ = cr.stroke();
        }
        ShapeKind::Rectangle => {
            cr.rectangle(x1.min(x2), y1.min(y2), (x1 - x2).abs(), (y1 - y2).abs());
            let _ = cr.stroke();
        }
        ShapeKind::Ellipse => {
            let (cx, cy) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
            let (rw, rh) = (((x1 - x2).abs() / 2.0).max(0.01), ((y1 - y2).abs() / 2.0).max(0.01));
            cr.save().ok();
            cr.translate(cx, cy);
            cr.scale(rw, rh);
            cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::PI * 2.0);
            cr.restore().ok();
            let _ = cr.stroke();
        }
        ShapeKind::Arrow => {
            cr.move_to(x1, y1);
            cr.line_to(x2, y2);
            let _ = cr.stroke();
            draw_arrow_head(cr, x2, y2, x1, y1, (block.width * 4.0).max(12.0));
        }
        ShapeKind::DoubleArrow => {
            cr.move_to(x1, y1);
            cr.line_to(x2, y2);
            let _ = cr.stroke();
            let head = (block.width * 4.0).max(12.0);
            draw_arrow_head(cr, x2, y2, x1, y1, head);
            draw_arrow_head(cr, x1, y1, x2, y2, head);
        }
    }
    cr.restore().ok();
}