use crate::models::color::*;
use crate::models::textbox::*;
use crate::models::image::*;
use crate::models::page::*;
use crate::models::stroke::*;
use crate::models::select::*;

use crate::translate_xournal::*;

use crate::save_handler::db::*;
use crate::save_handler::autosave;

use crate::gui::state::*;

use gtk::prelude::*;
use gtk::{Application, Window, Builder, CssProvider};
use gtk::gdk;
use gtk::cairo;

use glib::clone;
use glib::Propagation;

use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn component_bbox(payload: &ComponentPayload) -> Option<(f64, f64, f64, f64)> {
    match payload {
        ComponentPayload::PenStroke(s) | ComponentPayload::EraserStroke(s) => {
            if s.points.is_empty() { return None; }
            let hw = s.width / 2.0;
            let (mut mnx, mut mxx, mut mny, mut mxy) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for &(x, y) in &s.points {
                mnx = mnx.min(x - hw); mxx = mxx.max(x + hw);
                mny = mny.min(y - hw); mxy = mxy.max(y + hw);
            }
            Some((mnx, mny, mxx - mnx, mxy - mny))
        }
        ComponentPayload::RichText(b) => {
            let (mnx, mxx, mny, mxy) = b.approx_bbox();
            Some((mnx, mny, mxx - mnx, mxy - mny))
        }
        ComponentPayload::Image(b) => {
            let w = if b.width  > 0.0 { b.width  } else { 150.0 };
            let h = if b.height > 0.0 { b.height } else { 100.0 };
            Some((b.x, b.y, w, h))
        }
    }
}

pub fn draw_page(cr: &cairo::Context, page: &PageData, ox: f64, oy: f64, cache: &std::cell::RefCell<std::collections::HashMap<String, cairo::ImageSurface>>, bg: &PaperBackground) {
    
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
    cr.rectangle(ox + 5.0, oy + 5.0, PAGE_W, PAGE_H);
    let _ = cr.fill();
 
    
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.rectangle(ox, oy, PAGE_W, PAGE_H);
    let _ = cr.fill();
 
    
    match bg {
        PaperBackground::Ruled => {
            
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.85); 
            cr.set_line_width(0.8); 
            let mut ly = oy + LINE_SPACING;
            while ly < oy + PAGE_H - 10.0 {
                cr.move_to(ox + 10.0, ly);
                cr.line_to(ox + PAGE_W - 10.0, ly);
                let _ = cr.stroke();
                ly += LINE_SPACING;
            }
        }
        PaperBackground::Plain => {
            
        }
        PaperBackground::Grid => {
            const GRID_SIZE: f64 = 20.0; 
            
            cr.set_source_rgba(0.55, 0.55, 0.55, 0.75);
            cr.set_line_width(0.6); 
            
            
            let mut x = ox + GRID_SIZE;
            while x < ox + PAGE_W - 5.0 {
                cr.move_to(x, oy + 5.0);
                cr.line_to(x, oy + PAGE_H - 5.0);
                let _ = cr.stroke();
                x += GRID_SIZE;
            }
            
            let mut y = oy + GRID_SIZE;
            while y < oy + PAGE_H - 5.0 {
                cr.move_to(ox + 5.0, y);
                cr.line_to(ox + PAGE_W - 5.0, y);
                let _ = cr.stroke();
                y += GRID_SIZE;
            }
        }
    }

    
    cr.save().ok();
    cr.rectangle(ox, oy, PAGE_W, PAGE_H);
    cr.clip();
    cr.push_group();
 
    for component in &page.components {
        match component {
            
            ComponentPayload::Image(block) => {
                cr.set_operator(cairo::Operator::Over);
                render_image_block(cr, &block, ox, oy, cache); 
            }

            
            ComponentPayload::PenStroke(stroke) => {
                if stroke.points.len() < 2 { continue; }
                cr.set_operator(cairo::Operator::Over);
                cr.set_source_rgb(stroke.color.r, stroke.color.g, stroke.color.b);
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
 
            
            ComponentPayload::EraserStroke(stroke) => {
                if stroke.points.len() < 2 { continue; }
                cr.set_operator(cairo::Operator::Clear);
                cr.set_source_rgba(0.0, 0.0, 0.0, 1.0);
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
 
            
            ComponentPayload::RichText(block) => {
                
                cr.set_operator(cairo::Operator::Over);
                render_rich_text_block(cr, &block, ox, oy);
            }

        }
    }
 
    
    cr.set_operator(cairo::Operator::Over);
    cr.pop_group_to_source().ok();
    let _ = cr.paint();
    cr.restore().ok();
 
    
    cr.set_operator(cairo::Operator::Over);
    cr.set_source_rgba(0.45, 0.45, 0.45, 0.6);
    cr.set_line_width(1.0);
    cr.rectangle(ox, oy, PAGE_W, PAGE_H);
    let _ = cr.stroke();
}


pub fn setup_canvas_drawing(canvas: &gtk::DrawingArea, state: &Rc<RefCell<AppState>>) {
    let s = state.clone();
    canvas.connect_draw(move |widget, cr| {
        let alloc = widget.allocation();
        let w = alloc.width() as f64;
        
        
        cr.set_source_rgb(0.18, 0.18, 0.22);
        let _ = cr.paint();

        let zoom = s.borrow().zoom;
        let scroll_y = s.borrow().scroll_offset_y;

        
        let page_w_zoomed = PAGE_W * zoom;
        let ox_widget = ((w - page_w_zoomed) / 2.0).max(PAGE_MARGIN);
        let oy_widget = PAGE_MARGIN - scroll_y * zoom;

        
        s.borrow_mut().page_origin = (ox_widget, oy_widget);

        
        cr.save().ok();
        cr.translate(ox_widget, oy_widget);
        cr.scale(zoom, zoom);

        let st = s.borrow();
        
        
        draw_page(cr, &st.current_page_data, 0.0, 0.0, &st.image_cache, &st.current_page_data.background);
        
        
        if let Some(ref stroke) = st.current_stroke {
            
            draw_live_stroke(cr, stroke, &st.active_tool, 0.0, 0.0);
        }

        
        for &sel_idx in &st.selected_indices {
            draw_selection_overlay(cr, &st.current_page_data, sel_idx, 0.0, 0.0);
        }

        
        if let DragMode::Marquee { start_px, start_py, current_px, current_py } = st.drag_mode {
            cr.set_source_rgba(0.15, 0.5, 1.0, 0.3); 
            let min_x = start_px.min(current_px);
            let min_y = start_py.min(current_py);
            let w_rect = (start_px - current_px).abs();
            let h_rect = (start_py - current_py).abs();
            cr.rectangle(min_x, min_y, w_rect, h_rect);
            cr.fill().ok();
            
            cr.set_source_rgba(0.15, 0.5, 1.0, 0.8);
            cr.set_line_width(1.0);
            cr.rectangle(min_x, min_y, w_rect, h_rect);
            cr.stroke().ok();
        }

        cr.restore().ok();

        
        let needed_h = (PAGE_H * zoom + PAGE_MARGIN * 2.0) as i32;
        let needed_w = (PAGE_W * zoom + PAGE_MARGIN * 2.0) as i32;
        widget.set_size_request(needed_w, needed_h);

        Propagation::Proceed
    });
}