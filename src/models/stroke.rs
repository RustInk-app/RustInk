use crate::models::color;
use crate::models::page;

use gtk::cairo;

use serde::{Serialize, Deserialize};


pub const STROKE_THIN: f64   = 1.5;
pub const STROKE_MEDIUM: f64 = 3.0;
pub const STROKE_THICK: f64  = 6.0;


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stroke {
    pub points: Vec<(f64, f64)>,
    pub color:  color::Color,
    pub width:  f64,
}

pub fn draw_live_stroke(cr: &cairo::Context, stroke: &Stroke, tool: &page::Tool, ox: f64, oy: f64) {
    if stroke.points.len() < 2 { return; }
    match tool {
        page::Tool::Pen => {
            cr.set_operator(cairo::Operator::Over);
            cr.set_source_rgb(stroke.color.r, stroke.color.g, stroke.color.b);
        }
        page::Tool::Eraser => {
            cr.set_operator(cairo::Operator::Over);
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.4);
        }

        page::Tool::Text => return,
        page::Tool::Select => return,
        page::Tool::Shape(_) => return,
    }
    
    cr.set_line_width(stroke.width);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    let (sx, sy) = stroke.points[0];
    cr.move_to(ox + sx, oy + sy);
    for &(px, py) in &stroke.points[1..] {
        cr.line_to(ox + px, oy + py);
    }
    let _ = cr.stroke();
}
