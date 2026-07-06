use crate::gui::drawing::*;
use crate::models::page::*;
use gtk::cairo;

#[derive(Clone, Debug, PartialEq)]
pub enum ResizeHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Debug)]
pub enum DragMode {
    None,
    
    Move { start_px: f64, start_py: f64, orig_positions: Vec<(usize, f64, f64)> },
    Resize {
        handle:    ResizeHandle,
        orig_x:    f64,
        orig_y:    f64,
        orig_w:    f64,
        orig_h:    f64,
        start_px:  f64,
        start_py:  f64,
    },
    
    Marquee { start_px: f64, start_py: f64, current_px: f64, current_py: f64 },
}


pub fn hit_test_marquee(page: &PageData, mx: f64, my: f64, cx: f64, cy: f64) -> Vec<usize> {
    let min_x = mx.min(cx);
    let max_x = mx.max(cx);
    let min_y = my.min(cy);
    let max_y = my.max(cy);

    let mut selected = Vec::new();
    for (i, payload) in page.components.iter().enumerate() {
        if let Some((bx, by, bw, bh)) = component_bbox(payload) {
            
            if bx < max_x && bx + bw > min_x && by < max_y && by + bh > min_y {
                selected.push(i);
            }
        }
    }
    selected
}

const HANDLE_SIZE: f64 = 8.0;

pub fn hit_test_resize_handle(payload: &ComponentPayload, px: f64, py: f64) -> Option<ResizeHandle> {
    let (bx, by, bw, bh) = match payload {
        ComponentPayload::Image(b) => {
            let w = if b.width > 0.0 { b.width } else { 150.0 };
            let h = if b.height > 0.0 { b.height } else { 100.0 };
            (b.x, b.y, w, h)
        }
        ComponentPayload::RichText(b) => {
            let (mnx, mxx, mny, mxy) = b.approx_bbox();
            (mnx, mny, mxx - mnx, mxy - mny)
        }
        _ => return None,
    };

    let hs = HANDLE_SIZE;
    let corners = [
        (bx,      by,      ResizeHandle::TopLeft),
        (bx + bw, by,      ResizeHandle::TopRight),
        (bx,      by + bh, ResizeHandle::BottomLeft),
        (bx + bw, by + bh, ResizeHandle::BottomRight),
    ];
    for (cx, cy, handle) in corners {
        if px >= cx - hs && px <= cx + hs && py >= cy - hs && py <= cy + hs {
            return Some(handle);
        }
    }
    None
}

pub fn draw_selection_overlay(
    cr: &cairo::Context,
    page: &PageData,
    sel_idx: usize,
    ox: f64,
    oy: f64,
) {
    let payload = match page.components.get(sel_idx) {
        Some(p) => p,
        None    => return,
    };
    let (bx, by, bw, bh) = match component_bbox(payload) {
        Some(b) => b,
        None    => return,
    };

    cr.save().ok();
    cr.set_operator(cairo::Operator::Over);

    
    cr.set_source_rgba(0.15, 0.5, 1.0, 0.9);
    cr.set_line_width(1.5);
    cr.set_dash(&[6.0, 3.0], 0.0);
    cr.rectangle(ox + bx - 2.0, oy + by - 2.0, bw + 4.0, bh + 4.0);
    cr.stroke().ok();
    cr.set_dash(&[], 0.0);

    
    match payload {
        ComponentPayload::Image(_) | ComponentPayload::RichText(_) => {
            let hs = HANDLE_SIZE;
            let corners = [
                (bx,      by     ),
                (bx + bw, by     ),
                (bx,      by + bh),
                (bx + bw, by + bh),
            ];
            for (cx, cy) in corners {
                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
                cr.rectangle(ox + cx - hs / 2.0, oy + cy - hs / 2.0, hs, hs);
                cr.fill().ok();
                cr.set_source_rgba(0.15, 0.5, 1.0, 1.0);
                cr.set_line_width(1.5);
                cr.rectangle(ox + cx - hs / 2.0, oy + cy - hs / 2.0, hs, hs);
                cr.stroke().ok();
            }
        }
        _ => {}
    }

    cr.restore().ok();
}



fn dist_to_segment(px: f64, py: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let l2 = (x1 - x2).powi(2) + (y1 - y2).powi(2);
    if l2 == 0.0 { return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt(); }
    let t = (((px - x1) * (x2 - x1) + (py - y1) * (y2 - y1)) / l2).clamp(0.0, 1.0);
    let proj_x = x1 + t * (x2 - x1);
    let proj_y = y1 + t * (y2 - y1);
    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

pub fn hit_test_component(page: &PageData, px: f64, py: f64) -> Option<usize> {
    
    for (i, payload) in page.components.iter().enumerate().rev() {
        
        
        if let ComponentPayload::EraserStroke(_) = payload {
            continue;
        }

        match payload {
            ComponentPayload::PenStroke(stroke) => {
                let threshold = (stroke.width / 2.0) + 5.0; 
                if stroke.points.is_empty() { continue; }
                
                if stroke.points.len() == 1 {
                    let (x, y) = stroke.points[0];
                    if ((px - x).powi(2) + (py - y).powi(2)).sqrt() <= threshold {
                        return Some(i);
                    }
                } else {
                    for j in 0..stroke.points.len() - 1 {
                        let (x1, y1) = stroke.points[j];
                        let (x2, y2) = stroke.points[j+1];
                        if dist_to_segment(px, py, x1, y1, x2, y2) <= threshold {
                            return Some(i);
                        }
                    }
                }
            }
            
            ComponentPayload::RichText(_) | ComponentPayload::Image(_) | ComponentPayload::Shape(_) => {
                if let Some((bx, by, bw, bh)) = component_bbox(payload) {
                    if px >= bx && px <= bx + bw && py >= by && py <= by + bh {
                        return Some(i);
                    }
                }
            }
            _ => {}
        }
    }
    None
}
