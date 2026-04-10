use crate::image;
use crate::stroke;
use crate::textbox;

use serde::{Serialize, Deserialize};

pub const PAGE_W: f64 = 595.0;
pub const PAGE_H: f64 = 842.0;
pub const PAGE_MARGIN: f64 = 40.0;
pub const LINE_SPACING: f64 = 24.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Tool {
    Pen,
    Eraser,
    Text,
    Select,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ComponentPayload {
    PenStroke(stroke::Stroke),
    EraserStroke(stroke::Stroke),
    RichText(textbox::RichTextBlock),
    Image(image::ImageBlock),
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
}

impl PageData {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
            background: PaperBackground::Grid, 
        }
    }
}