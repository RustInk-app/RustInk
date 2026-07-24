use gtk::prelude::*;
use gtk::gdk;
use gtk::cairo;

use serde::{Serialize, Deserialize};

use crate::save_handler::autosave::*;
use crate::save_handler::autosave_utilities::*;


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageBlock {
    pub filename: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn render_image_block(
    cr:    &cairo::Context,
    block: &ImageBlock,
    ox:    f64,
    oy:    f64,
    cache: &std::cell::RefCell<std::collections::HashMap<String, cairo::ImageSurface>>,
) {
    let fname = std::path::Path::new(&block.filename)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let mut cache_mut = cache.borrow_mut();

    
    if !cache_mut.contains_key(&fname) {
        let webp_path = media_dir().join(&fname);
        
        if let Ok(bytes) = std::fs::read(&webp_path) {
            if let Ok(img) = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP) {
                let img_rgba = img.to_rgba8();
                let iw = img_rgba.width() as i32;
                let ih = img_rgba.height() as i32;

                if let Ok(mut surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, iw, ih) {
                    let stride = surface.stride() as usize;
                    let pixels = img_rgba.as_raw();
                    if let Ok(mut data) = surface.data() {
                        for y in 0..ih as usize {
                            for x in 0..iw as usize {
                                let src = (y * iw as usize + x) * 4;
                                let dst = y * stride + x * 4;
                                data[dst]     = pixels[src + 2]; 
                                data[dst + 1] = pixels[src + 1]; 
                                data[dst + 2] = pixels[src];     
                                data[dst + 3] = pixels[src + 3]; 
                            }
                        }
                    }
                    
                    cache_mut.insert(fname.clone(), surface);
                }
            }
        }
    }

    
    if let Some(surface) = cache_mut.get(&fname) {
        let iw = surface.width() as f64;
        let ih = surface.height() as f64;
        let dw = if block.width > 0.0 { block.width } else { iw };
        let dh = if block.height > 0.0 { block.height } else { ih };

        cr.save().ok();
        cr.translate(ox + block.x, oy + block.y);
        cr.scale(dw / iw, dh / ih);
        if cr.set_source_surface(surface, 0.0, 0.0).is_ok() {
            cr.paint().ok();
        }
        cr.restore().ok();
    } else {
        
        let dw = if block.width  > 0.0 { block.width  } else { 150.0 };
        let dh = if block.height > 0.0 { block.height } else { 100.0 };
        cr.save().ok();
        cr.set_source_rgba(0.85, 0.85, 0.85, 0.8);
        cr.rectangle(ox + block.x, oy + block.y, dw, dh);
        cr.fill().ok();
        cr.set_source_rgba(0.5, 0.5, 0.5, 1.0);
        cr.set_line_width(1.0);
        cr.rectangle(ox + block.x + 0.5, oy + block.y + 0.5, dw - 1.0, dh - 1.0);
        cr.stroke().ok();
        cr.restore().ok();
    }
}

pub fn detect_image_format(bytes: &[u8]) -> Option<image::ImageFormat> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(image::ImageFormat::Jpeg); 
    }
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(image::ImageFormat::Png);  
    }
    None 
}

pub fn show_format_error_dialog(parent: &gtk::Window) {
    let dialog = gtk::MessageDialog::new(
        Some(parent),
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Error,
        gtk::ButtonsType::Ok,
        "Formato non supportato",
    );
    dialog.set_secondary_text(Some(
        "Puoi incollare solo immagini nei formati JPEG o PNG.\n\
         I formati GIF, SVG e altri non sono supportati."
    ));
    dialog.run();
    unsafe { dialog.destroy(); }
}

pub fn pixbuf_to_raw_bytes(pixbuf: &gdk::gdk_pixbuf::Pixbuf) -> Option<Vec<u8>> {
    pixbuf.save_to_bufferv("png", &[]).ok()
}
