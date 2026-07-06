use crate::models::page::*;
use crate::models::select::*;
use crate::save_handler::db::encode_payload_list;

use gtk::prelude::*;
use gtk::gdk;
use glib::Propagation;
use glib::clone;

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
    
    window.connect_key_press_event(clone!(@strong s as state, @strong c as canvas, @strong w as window => move |_, event| {
        use gtk::gdk::keys::constants as Key;
        let mods = event.state();
        let key  = event.keyval();
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);

        // --- INIZIO HOLD-TO-SWITCH PER I TASTI ---
        let key_name = key.name().unwrap_or_default().to_string();
        let key_trigger = EventTrigger::Key(key_name);
        
        let target_tool = {
            let st = state.borrow();
            if Some(&key_trigger) == st.pref_trigger_1.as_ref() { st.pref_tool_1.clone() }
            else if Some(&key_trigger) == st.pref_trigger_2.as_ref() { st.pref_tool_2.clone() }
            else { None }
        };

        if let Some(tool) = target_tool {
            let (needs_switch, current, cb) = {
                let st = state.borrow();
                if st.active_temp_trigger.is_none() && st.active_tool != tool {
                    (true, st.active_tool.clone(), st.update_toolbar_ui.clone())
                } else {
                    (false, st.active_tool.clone(), None)
                }
            };
            
            if needs_switch {
                let mut st_mut = state.borrow_mut();
                st_mut.previous_tool = Some(current);
                st_mut.active_tool = tool.clone();
                st_mut.active_temp_trigger = Some(key_trigger);
                drop(st_mut);
                
                if let Some(f) = cb { f(&tool); }
                canvas.queue_draw();
            }
            return Propagation::Stop;
        }
        // --- FINE HOLD-TO-SWITCH ---

        // --- INIZIO SCORCIATOIE CLASSICHE ---
        if ctrl && key == Key::z {
            state.borrow_mut().undo();
            let title = state.borrow().window_title();
            window.set_title(&title);
            canvas.queue_draw();
            return Propagation::Stop;
        }

        if ctrl && (key == Key::y || key == Key::Y) {
            state.borrow_mut().redo();
            let title = state.borrow().window_title();
            window.set_title(&title);
            canvas.queue_draw();
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
                        show_format_error_dialog(&window);
                        return Propagation::Stop;
                    }
                };
                
                let fmt = detect_image_format(&raw_bytes);
                if fmt.is_none() {
                    show_format_error_dialog(&window);
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
                        show_format_error_dialog(&window);
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
                    let mut st = state.borrow_mut();
                    st.commit_component(ComponentPayload::Image(block));
                    let title = st.window_title();
                    drop(st);
                    window.set_title(&title);
                }
                canvas.queue_draw();
            }
            return Propagation::Stop;
        }
        
        if key == Key::Delete || key == Key::BackSpace {
            let has_selection = !state.borrow().selected_indices.is_empty();
            if has_selection {
                let mut st = state.borrow_mut();
                
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
                window.set_title(&title);
                canvas.queue_draw();
                return Propagation::Stop;
            }
        }

        if ctrl && (key == Key::a || key == Key::A) {
            let mut st = state.borrow_mut();
            st.active_tool = Tool::Select;
            let count = st.current_page_data.components.len();
            st.selected_indices = (0..count).collect(); 
            drop(st);
            canvas.queue_draw();
            return Propagation::Stop;
        }

        Propagation::Proceed
    }));

    // --- INIZIO RILASCIO TASTO (HOLD-TO-SWITCH) ---
    window.connect_key_release_event(clone!(@strong s as state, @strong c as canvas => move |_, event| {
        let key_name = event.keyval().name().unwrap_or_default().to_string();
        let key_trigger = EventTrigger::Key(key_name);

        let st = state.borrow();
        if st.active_temp_trigger.as_ref() == Some(&key_trigger) {
            let prev = st.previous_tool.clone();
            let cb = st.update_toolbar_ui.clone();
            drop(st);
            
            let mut st_mut = state.borrow_mut();
            if let Some(p) = &prev {
                st_mut.active_tool = p.clone();
            }
            st_mut.previous_tool = None;
            st_mut.active_temp_trigger = None;
            drop(st_mut);

            if let Some(p) = prev {
                if let Some(f) = cb { f(&p); }
            }
            canvas.queue_draw();
            return Propagation::Stop;
        }
        Propagation::Proceed
    }));
    // --- FINE RILASCIO TASTO ---
}