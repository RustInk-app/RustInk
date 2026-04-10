

use crate::models::page::*;
use crate::models::select::*;
use crate::save_handler::db::encode_payload_list;

use gtk::prelude::*;
use gtk::gdk;
use glib::Propagation;

use std::cell::RefCell;
use std::rc::Rc;

use crate::gui::state::*;

use crate::models::image::*;

use crate::save_handler::autosave::*;

use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn setup_keyboard_shortcuts(
    window: &gtk::Window,
    state: &Rc<RefCell<AppState>>,
    canvas: &gtk::DrawingArea,
) {
    let s  = state.clone();
    let c  = canvas.clone();
    let w  = window.clone();
    
    window.connect_key_press_event(move |_, event| {
        use gtk::gdk::keys::constants as Key;
        let mods = event.state();
        let key  = event.keyval();
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);

        if ctrl && key == Key::z {
            s.borrow_mut().undo();
            let title = s.borrow().window_title();
            w.set_title(&title);
            c.queue_draw();
            return Propagation::Stop;
        }
        if ctrl && (key == Key::y || key == Key::Y) {
            s.borrow_mut().redo();
            let title = s.borrow().window_title();
            w.set_title(&title);
            c.queue_draw();
            return Propagation::Stop;
        }
        
        if ctrl && (key == Key::v || key == Key::V) {
            let clipboard = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD);

            
            if !clipboard.wait_is_image_available() {
                return Propagation::Stop;
            }

            if let Some(pixbuf) = clipboard.wait_for_image() {
                
                let raw_bytes = match pixbuf_to_raw_bytes(&pixbuf) {
                    Some(b) => b,
                    None    => {
                        show_format_error_dialog(&w);
                        return Propagation::Stop;
                    }
                };

                
                let fmt = detect_image_format(&raw_bytes);
                if fmt.is_none() {
                    show_format_error_dialog(&w);
                    return Propagation::Stop;
                }

                
                let media_tmp = media_dir();
                let _ = std::fs::create_dir_all(&media_tmp);

                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let webp_filename = format!("img_{ts}.webp");
                let webp_path     = media_tmp.join(&webp_filename);
                let bundle_entry  = format!("media/{webp_filename}");

                
                let img = match image::load_from_memory(&raw_bytes) {
                    Ok(i)  => i,
                    Err(_) => {
                        show_format_error_dialog(&w);
                        return Propagation::Stop;
                    }
                };

                let iw_orig = img.width()  as f64;
                let ih_orig = img.height() as f64;

                
                if let Err(e) = img.save_with_format(&webp_path, image::ImageFormat::WebP) {
                    eprintln!("[PASTE] Errore salvataggio WebP: {e}");
                    return Propagation::Stop;
                }

                
                
                let max_w = PAGE_W * 0.90;
                let max_h = PAGE_H * 0.90;
                let scale = (max_w / iw_orig).min(max_h / ih_orig).min(1.0);
                let iw = iw_orig * scale;
                let ih = ih_orig * scale;

                
                let x = (PAGE_W / 2.0 - iw / 2.0).max(0.0);
                let y = (PAGE_H / 2.0 - ih / 2.0).max(0.0);

                let block = ImageBlock {
                    filename: bundle_entry,
                    x,
                    y,
                    width:  iw,
                    height: ih,
                };

                {
                    let mut st = s.borrow_mut();
                    st.commit_component(ComponentPayload::Image(block));
                    let title = st.window_title();
                    drop(st);
                    w.set_title(&title);
                }
                c.queue_draw();
            }
            return Propagation::Stop;
        }

        
        if key == Key::Delete || key == Key::BackSpace {
            let has_selection = !s.borrow().selected_indices.is_empty();
            if has_selection {
                let mut st = s.borrow_mut();

                
                let mut indices = st.selected_indices.clone();
                indices.sort_unstable_by(|a, b| b.cmp(a));
                for idx in &indices {
                    if *idx < st.current_page_data.components.len() {
                        st.current_page_data.components.remove(*idx);
                    }
                }
                st.selected_indices.clear();
                st.selected_index = None;
                st.drag_mode = DragMode::None;

                
                if let Some(conn) = &st.db {
                    let blob = encode_payload_list(&st.current_page_data.components);
                    let _ = conn.execute(
                        "DELETE FROM component_rtree WHERE id IN \
                         (SELECT id FROM active_components WHERE page_id = ?1)",
                        rusqlite::params![st.current_page_id],
                    );
                    let _ = conn.execute(
                        "DELETE FROM active_components WHERE page_id = ?1",
                        rusqlite::params![st.current_page_id],
                    );
                    let _ = conn.execute(
                        "UPDATE base_layers SET baked_blob = ?1 WHERE page_id = ?2",
                        rusqlite::params![blob, st.current_page_id],
                    );
                }

                st.is_modified = true;
                st.undo_stack.clear();
                st.redo_stack.clear();
                let title = st.window_title();
                drop(st);
                w.set_title(&title);
                c.queue_draw();
                return Propagation::Stop;
            }
        }

        if ctrl && (key == Key::a || key == Key::A) {
            let mut st = s.borrow_mut();
            st.active_tool = Tool::Select;
            let count = st.current_page_data.components.len();
            st.selected_indices = (0..count).collect(); 
            drop(st);
            c.queue_draw();
            return Propagation::Stop;
        }

        Propagation::Proceed;
        gtk::glib::Propagation::Proceed
    });
}