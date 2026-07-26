use crate::models::textbox::*;
use crate::models::image::*;
use crate::models::page::*;
use crate::models::stroke::*;
use crate::models::select::*;

use crate::gui::state::*;
use glib::{Propagation, ControlFlow, clone};

use gtk::prelude::*;
use gtk::cairo;

use std::cell::RefCell;
use std::rc::Rc;
use cairo::Context;

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
        ComponentPayload::Shape(b) => {
            let hw = b.width / 2.0 + 4.0;
            let mnx = b.x1.min(b.x2) - hw;
            let mxx = b.x1.max(b.x2) + hw;
            let mny = b.y1.min(b.y2) - hw;
            let mxy = b.y1.max(b.y2) + hw;
            Some((mnx, mny, mxx - mnx, mxy - mny))
        }
    }
}

pub fn draw_page(
    cr: &cairo::Context,
    page: &PageData,
    ox: f64,
    oy: f64,
    zoom: f64, // Nuovo parametro
    cache: &std::cell::RefCell<std::collections::HashMap<String, cairo::ImageSurface>>,
    bg: &PaperBackground,
    pdf_bg: Option<(&poppler::Document, i64, i64)>, // doc, doc_id, page_index
    pdf_surface_cache: &std::cell::RefCell<std::collections::HashMap<(i64, i64, u32), cairo::ImageSurface>>, // Nuovo parametro
)
{
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
    cr.rectangle(ox + 5.0, oy + 5.0, PAGE_W, PAGE_H);
    let _ = cr.fill();

    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.rectangle(ox, oy, PAGE_W, PAGE_H);
    let _ = cr.fill();

    match pdf_bg 
    {
        Some((doc, doc_id, page_index)) => 
        {
            // Il PDF viene disegnato usando il sistema di cache ad alte prestazioni
            draw_pdf_background(cr, doc, doc_id, page_index, ox, oy, zoom, pdf_surface_cache);
        }
        None => 
        {
            match bg 
            {
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
                render_rich_text_block(cr, &block, ox, oy, cache);
            }
            ComponentPayload::Shape(block) => {
                cr.set_operator(cairo::Operator::Over);
                render_shape(cr, block, ox, oy);
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

/// Disegna la pagina `page_index` del documento poppler `doc` scalata per
/// riempire esattamente il riquadro (PAGE_W x PAGE_H), all'origine (ox, oy).
/// Questo è vettoriale: nessuna rasterizzazione manuale, poppler disegna
/// direttamente sul cairo::Context passato.
pub fn draw_pdf_background(
    cr: &Context,
    doc: &poppler::Document,
    doc_id: i64,
    page_index: i64,
    ox: f64,
    oy: f64,
    zoom: f64,
    surface_cache: &std::cell::RefCell<std::collections::HashMap<(i64, i64, u32), cairo::ImageSurface>>,
) {
    let Some(pdf_page) = doc.page(page_index as i32) else { return };
    let (pw, ph) = pdf_page.size();
    if pw <= 0.0 || ph <= 0.0 { return; }

    // --- 1. IL VERO SEGRETO: Due sole risoluzioni fisse! ---
    // Se lo zoom è molto piccolo (es. Sidebar), creiamo una miniatura.
    // Altrimenti creiamo un'immagine ad alta risoluzione (Canvas).
    let is_thumbnail = zoom < 0.5;
    let zoom_key = if is_thumbnail { 0 } else { 1 };
    
    // La chiave della cache ora NON cambia ai micro-movimenti dello zoom!
    let cache_key = (doc_id, page_index, zoom_key);

    let mut cache = surface_cache.borrow_mut();

    // --- 2. Limite alzato a 150 ---
    // Questo permette di scorrere agevolmente PDF da decine e decine 
    // di pagine nella sidebar senza mai svuotare la cache!
    if cache.len() > 150 {
        cache.clear();
    }

    let surface = cache.entry(cache_key).or_insert_with(|| {
        // Scala: Sidebar = microscopico (0.2x). Canvas = Alta risoluzione fissa (2.0x).
        let render_scale = if is_thumbnail { 0.2 } else { 2.0 };
        
        let target_w = (PAGE_W * render_scale) as i32;
        let target_h = (PAGE_H * render_scale) as i32;

        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, target_w, target_h)
            .expect("Impossibile creare la superficie PDF cacheata");

        let ctx = cairo::Context::new(&surface).unwrap();
        ctx.set_source_rgb(1.0, 1.0, 1.0);
        ctx.paint().unwrap();

        let scale_x = (target_w as f64) / pw;
        let scale_y = (target_h as f64) / ph;
        ctx.scale(scale_x, scale_y);

        // Questo render pesante ora avviene al massimo DUE volte per pagina in tutta la sessione!
        pdf_page.render(&ctx);

        surface
    });

    cr.save().ok();
    cr.translate(ox, oy);

    // --- 3. Hardware Scaling In Tempo Reale ---
    // Dato che il nostro context grafico (cr) è GIA' influenzato dallo zoom globale 
    // (nel setup del canvas), dobbiamo solo "riportare" l'immagine alla dimensione base di PAGE_W.
    // Cairo scalerà la bitmap fisicamente. È un'operazione fulminea!
    let render_scale = if is_thumbnail { 0.2 } else { 2.0 };
    cr.scale(1.0 / render_scale, 1.0 / render_scale);

    cr.set_source_surface(surface, 0.0, 0.0).unwrap();
    cr.paint().unwrap();

    cr.restore().ok();
}

pub fn setup_canvas_drawing(canvas: &gtk::DrawingArea, state: &Rc<RefCell<AppState>>) {
    
    let s = state.clone();
    let last_size = Rc::new(RefCell::new((0, 0)));

    canvas.connect_draw(move |widget, cr| {
        let alloc = widget.allocation();
        let w = alloc.width() as f64;
        
        // Sfondo grigio della finestra di base
        cr.set_source_rgb(0.18, 0.18, 0.22);
        let _ = cr.paint();

        let zoom = s.borrow().zoom;
        
        // --- NUOVO: Margine fisso decorativo tra il foglio e l'interfaccia ---
        let margin = 40.0; 
        
        let page_w_zoomed = PAGE_W * zoom;
        let page_h_zoomed = PAGE_H * zoom;
        
        // L'origine Y è sempre ancorata al margine. Sarà GTK a muovere tutto in alto/basso!
        let ox_widget = ((w - page_w_zoomed) / 2.0).max(margin);
        let oy_widget = margin; 

        s.borrow_mut().page_origin = (ox_widget, oy_widget);

        cr.save().ok();
        cr.translate(ox_widget, oy_widget);
        cr.scale(zoom, zoom);

        let st = s.borrow();

        // Risolvi lo sfondo PDF estraendo anche il doc_id (necessario per la chiave di cache)
        let pdf_bg = st.current_pdf_ref.and_then(|pref| {
            let cache = st.pdf_cache.borrow();
            cache.get(&pref.doc_id).cloned().map(|doc| (doc, pref.doc_id, pref.page_index))
        });

        draw_page(
            cr,
            &st.current_page_data,
            0.0, 0.0,
            zoom,
            &st.image_cache,
            &st.current_page_data.background,
            pdf_bg.as_ref().map(|(doc, did, idx)| (doc, *did, *idx)),
            &st.pdf_surface_cache,
        );

        if let Some(ref stroke) = st.current_stroke {
            draw_live_stroke(cr, stroke, &st.active_tool, 0.0, 0.0);
        }
        
        if let Some(ref shape) = st.current_shape {
            render_shape(cr, shape, 0.0, 0.0);
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

        let needed_h = (page_h_zoomed + margin * 2.0) as i32;
        let needed_w = (page_w_zoomed + margin * 2.0) as i32;
        
        let mut last = last_size.borrow_mut();
        
        // Richiediamo a GTK di aggiornare le scrollbar SOLO se la dimensione è fisicamente cambiata
        if last.0 != needed_w || last.1 != needed_h {
            *last = (needed_w, needed_h);
            
            // glib::idle_add_local dice a GTK: "Appena hai finito di renderizzare questo fotogramma 
            // e sei a riposo, aggiorna le dimensioni". Questo spezza il loop!
            glib::idle_add_local(clone!(@weak widget => @default-return ControlFlow::Break, move || {
                widget.set_size_request(needed_w, needed_h);
                ControlFlow::Break
            }));
        }

        Propagation::Proceed
    });
}