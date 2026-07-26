use crate::models::color;

use serde::{Serialize, Deserialize};

use pango::{AttrList, AttrColor, AttrFontDesc, FontDescription, WrapMode};

use gtk::prelude::*;
use gtk::gdk;
use gtk::cairo;

use glib::Propagation;

use std::cell::RefCell;
use std::rc::Rc;

const TEXT_PADDING: f64 = 2.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextStyle {
    pub font_family: String,
    pub size:        f64,
    pub color:       color::Color,
    pub bold:        bool,
    pub italic:      bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: "Sans".into(),
            size:        12.0,
            color:       color::Color::black(),
            bold:        false,
            italic:      false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextSpan {
    pub text:  String,
    pub style: TextStyle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RichTextBlock {
    pub id_temporaneo: String,
    pub x:     f64,
    pub y:     f64,
    pub width: f64,
    pub spans: Vec<TextSpan>,
}

impl RichTextBlock {
    
    pub fn approx_bbox(&self) -> (f64, f64, f64, f64) {
        let total_chars: usize = self.spans.iter().map(|s| s.text.len()).sum();
    
        let avg_size = self.spans.first()
            .map(|s| s.style.size)
            .unwrap_or(12.0);
        let line_h   = avg_size * 1.6;                         
        let chars_per_row = ((self.width / (avg_size * 0.55)).ceil() as usize).max(1);
        let rows     = ((total_chars + chars_per_row - 1) / chars_per_row).max(1);
        let height   = rows as f64 * line_h;

        (self.x, self.x + self.width, self.y, self.y + height)
    }
}

pub fn render_rich_text_block(
    cr:    &cairo::Context,
    block: &RichTextBlock,
    ox:    f64,
    oy:    f64,
    cache: &RefCell<std::collections::HashMap<String, cairo::ImageSurface>>
) {
    let mut cache_mut = cache.borrow_mut();
    // Usiamo l'ID univoco del blocco come chiave di cache
    let cache_key = format!("txt_{}", block.id_temporaneo);

    // Se l'immagine del testo non è in cache, la creiamo!
    if !cache_mut.contains_key(&cache_key) {
        // 1. Creiamo un contesto temporaneo minuscolo solo per calcolare gli spazi
        let tmp_surface = cairo::ImageSurface::create(cairo::Format::A8, 1, 1).unwrap();
        let tmp_cr = cairo::Context::new(&tmp_surface).unwrap();
        let layout = pangocairo::create_layout(&tmp_cr);

        layout.set_width((block.width * pango::SCALE as f64) as i32);
        layout.set_wrap(WrapMode::Word);

        let mut full_text   = String::new();
        let attr_list   = AttrList::new();
        let mut byte_offset = 0u32;
        
        for span in &block.spans {
            let start = byte_offset;
            let end   = byte_offset + span.text.len() as u32;

            let mut fd = FontDescription::new();
            fd.set_family(&span.style.font_family);
            fd.set_size((span.style.size * pango::SCALE as f64) as i32);
            fd.set_weight(if span.style.bold { pango::Weight::Bold } else { pango::Weight::Normal });
            fd.set_style(if span.style.italic { pango::Style::Italic } else { pango::Style::Normal });

            let mut attr_font = AttrFontDesc::new(&fd);
            attr_font.set_start_index(start);
            attr_font.set_end_index(end);
            attr_list.insert(attr_font);

            let r16 = (span.style.color.r * 65535.0) as u16;
            let g16 = (span.style.color.g * 65535.0) as u16;
            let b16 = (span.style.color.b * 65535.0) as u16;

            let mut attr_color = AttrColor::new_foreground(r16, g16, b16);
            attr_color.set_start_index(start);
            attr_color.set_end_index(end);
            attr_list.insert(attr_color);

            full_text.push_str(&span.text);
            byte_offset = end;
        }

        layout.set_text(&full_text);
        layout.set_attributes(Some(&attr_list));

        // 2. Chiediamo a Pango le vere dimensioni in pixel del testo formattato
        let (_, logical_rect) = layout.pixel_extents();
        let real_w = (logical_rect.width() as f64 + TEXT_PADDING * 2.0).max(1.0);
        let real_h = (logical_rect.height() as f64 + TEXT_PADDING * 2.0).max(1.0);

        // 3. Creiamo la Surface finale in ALTA RISOLUZIONE (2.0x per nitidezza)
        let render_scale = 2.0;
        let target_w = (real_w * render_scale).ceil() as i32;
        let target_h = (real_h * render_scale).ceil() as i32;

        if let Ok(surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, target_w, target_h) {
            let final_cr = cairo::Context::new(&surface).unwrap();
            final_cr.scale(render_scale, render_scale);
            final_cr.move_to(TEXT_PADDING, TEXT_PADDING);
            
            // Colleghiamo il layout al nuovo context reale e lo disegniamo
            pangocairo::update_layout(&final_cr, &layout);
            pangocairo::show_layout(&final_cr, &layout);

            // Salviamo la rasterizzazione in cache
            cache_mut.insert(cache_key.clone(), surface);
        }
    }

    // 4. DISEGNO FULMINEO: Recuperiamo l'immagine dalla cache e la posizioniamo!
    if let Some(surface) = cache_mut.get(&cache_key) {
        cr.save().ok();
        cr.translate(ox + block.x, oy + block.y);
        cr.scale(0.5, 0.5); // Compensiamo il moltiplicatore 2.0x usato per la nitidezza
        cr.set_source_surface(surface, 0.0, 0.0).unwrap();
        cr.paint().unwrap();
        cr.restore().ok();
    }
}


pub fn show_text_input_dialog(
    parent:        &gtk::Window,
    default_style: &TextStyle,
    initial_text: &str
) -> Option<(String, TextStyle)> {
 
    
    let dialog = gtk::Dialog::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_title("Inserisci testo");
    dialog.set_default_size(480, 320);
 
    dialog.add_button("Annulla", gtk::ResponseType::Cancel);
    dialog.set_default_response(gtk::ResponseType::Ok);
 
    let content = dialog.content_area();
    content.set_spacing(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
 
    
    let row_settings = gtk::Box::new(gtk::Orientation::Horizontal, 6);
 
    
    
    let lbl_font = gtk::Label::new(Some("Font:"));
    
    
    let combo_font = gtk::ComboBoxText::with_entry();
    combo_font.set_tooltip_text(Some("Cerca o seleziona un font"));

    
    let pango_ctx = dialog.pango_context();
    let mut family_names: Vec<String> = pango_ctx
        .list_families()
        .into_iter()
        .map(|f| f.name().to_string())
        .collect();
    
    
    family_names.sort_by_key(|n| n.to_lowercase());
    family_names.dedup();

    
    for name in &family_names {
        combo_font.append_text(name);
    }

    
    let entry_font = combo_font
        .child()
        .expect("ComboBoxText non ha un child")
        .downcast::<gtk::Entry>()
        .expect("Il child non è una Entry");
        
    entry_font.set_text(&default_style.font_family);
    entry_font.set_width_chars(12);

    
    if let Some(model) = combo_font.model() {
        let completion = gtk::EntryCompletion::new();
        completion.set_model(Some(&model));
        completion.set_text_column(0);
        completion.set_inline_completion(true);
        completion.set_popup_completion(true);
        
        
        
        completion.set_match_func(|completion, key, iter| {
            if let Some(model) = completion.model() {
                let value = model.value(iter, 0);
                if let Ok(text) = value.get::<String>() {
                    return text.to_lowercase().contains(&key.to_lowercase());
                }
            }
            false
        });
        
        entry_font.set_completion(Some(&completion));
    }
 
    
    let lbl_size = gtk::Label::new(Some("Dim:"));
    let spin_size = gtk::SpinButton::with_range(6.0, 144.0, 1.0);
    spin_size.set_value(default_style.size);
    spin_size.set_tooltip_text(Some("Dimensione in punti"));
    spin_size.set_width_chars(5);
 
    
    let chk_bold = gtk::CheckButton::with_label("G");
    chk_bold.set_active(default_style.bold);
    chk_bold.set_tooltip_text(Some("Grassetto"));
 
    
    let chk_italic = gtk::CheckButton::with_label("I");
    chk_italic.set_active(default_style.italic);
    chk_italic.set_tooltip_text(Some("Corsivo"));
 
    
    let color_cell = gtk::DrawingArea::new();
    color_cell.set_size_request(24, 24);
    
    let chosen_color = Rc::new(RefCell::new(default_style.color.clone()));
    {
        let cc = chosen_color.clone();
        color_cell.connect_draw(move |_, cr| {
            let c = cc.borrow();
            cr.set_source_rgb(c.r, c.g, c.b);
            cr.rectangle(0.0, 0.0, 24.0, 24.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.3, 0.3, 0.3);
            cr.set_line_width(1.0);
            cr.rectangle(0.5, 0.5, 23.0, 23.0);
            let _ = cr.stroke();
            Propagation::Proceed
        });
    }
    let btn_color = gtk::Button::new();
    btn_color.set_tooltip_text(Some("Scegli colore testo"));
    btn_color.add(&color_cell);
    {
        let cc  = chosen_color.clone();
        let ca  = color_cell.clone();
        let dlg = dialog.clone();
        btn_color.connect_clicked(move |_| {
            let cd = gtk::ColorChooserDialog::new(Some("color::Colore testo"), Some(&dlg));
            let cur = cc.borrow();
            cd.set_rgba(&gdk::RGBA::new(cur.r, cur.g, cur.b, 1.0));
            drop(cur);
            if cd.run() == gtk::ResponseType::Ok {
                let rgba = cd.rgba();
                *cc.borrow_mut() = color::Color::new(rgba.red(), rgba.green(), rgba.blue());
                ca.queue_draw();
            }
            unsafe { cd.destroy(); }
        });
    }
 
    row_settings.pack_start(&lbl_font,   false, false, 0);
    row_settings.pack_start(&combo_font, false, false, 0);
    row_settings.pack_start(&lbl_size,   false, false, 4);
    row_settings.pack_start(&spin_size,  false, false, 0);
    row_settings.pack_start(&chk_bold,   false, false, 4);
    row_settings.pack_start(&chk_italic, false, false, 0);
    row_settings.pack_start(&btn_color,  false, false, 6);
 
    content.pack_start(&row_settings, false, false, 0);
 
    
    let scrolled_text = gtk::ScrolledWindow::new(
        None::<&gtk::Adjustment>,
        None::<&gtk::Adjustment>,
    );
    scrolled_text.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scrolled_text.set_vexpand(true);
 
    let text_view = gtk::TextView::new();
    text_view.set_wrap_mode(gtk::WrapMode::Word);
    text_view.set_left_margin(6);
    text_view.set_right_margin(6);
    text_view.set_top_margin(4);
    text_view.set_bottom_margin(4);
    text_view.set_accepts_tab(false); 
 
    text_view.set_accepts_tab(true);
    
    if !initial_text.is_empty() {
        text_view.buffer().unwrap().set_text(initial_text);
    }
    
    {
        let dlg = dialog.clone();
        text_view.connect_key_press_event(move |_, event| {
            use gdk::keys::constants as Key;
            let mods = event.state();
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            if ctrl && event.keyval() == Key::Return {
                dlg.response(gtk::ResponseType::Ok);
                return Propagation::Stop;
            }
            Propagation::Proceed
        });
    }
 
    scrolled_text.add(&text_view);
    content.pack_start(&scrolled_text, true, true, 0);
 
    
    let lbl_hint = gtk::Label::new(Some("Ctrl+Invio per confermare"));
    lbl_hint.set_halign(gtk::Align::End);
    {
        let ctx = lbl_hint.style_context();
        ctx.add_class("dim-label");
    }
    content.pack_start(&lbl_hint, false, false, 0);
 
    dialog.show_all();
    text_view.grab_focus();
 
    
    let response = dialog.run();
    
    
    let result = if response == gtk::ResponseType::Ok {
        let buffer = text_view.buffer().unwrap();
        
        let text = buffer.text(
            &buffer.start_iter(),
            &buffer.end_iter(),
            false,
        ).map(|s| s.to_string()).unwrap_or_default();
 
        if text.trim().is_empty() {
            None
        } else {
            let style = TextStyle {
                font_family: entry_font.text().to_string(),
                size:        spin_size.value(),
                color:       chosen_color.borrow().clone(),
                bold:        chk_bold.is_active(),
                italic:      chk_italic.is_active(),
            };
            Some((text, style))
        }
    } else {
        None
    };
 
    unsafe { dialog.destroy(); }
    result
}
