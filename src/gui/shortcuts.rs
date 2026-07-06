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

use gdk::keys::constants as keys;
use crate::models::page::{PAGE_W, PAGE_H};

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
        if ctrl && (key == Key::z || key == Key::Z) {
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
        
        if ctrl && (key == keys::c || key == keys::C) 
        {
            // --- CTRL + C : COPIA ---
            let st = state.borrow();
            // Diamo il tipo esplicito a Rust per evitare errori di compilazione
            let mut copied: Vec<crate::models::page::ComponentPayload> = Vec::new();
            
            // Copia gli elementi dalla selezione multipla
            for &idx in &st.selected_indices {
                if let Some(comp) = st.current_page_data.components.get(idx) {
                    copied.push(comp.clone());
                }
            }
            
            // Per sicurezza: se usi un clic singolo senza selezioni multiple
            if let Some(idx) = st.selected_index {
                if !st.selected_indices.contains(&idx) {
                    if let Some(comp) = st.current_page_data.components.get(idx) {
                        copied.push(comp.clone());
                    }
                }
            }
            
            drop(st);
            state.borrow_mut().clipboard = copied;
            return Propagation::Stop;
        }
            
        if ctrl && (key == keys::v || key == keys::V) {
            // --- CTRL + V : INCOLLA ---
            let mut st = state.borrow_mut();
            if st.clipboard.is_empty() {
                return Propagation::Proceed;
            }

            // 1. Calcola l'ingombro massimo e minimo degli elementi nella clipboard
            let mut min_x = f64::MAX;
            let mut min_y = f64::MAX;
            let mut max_x = f64::MIN;
            let mut max_y = f64::MIN;

            for comp in &st.clipboard {
                let (cx1, cx2, cy1, cy2) = crate::save_handler::db::bounding_box(comp);
                if cx1 < min_x { min_x = cx1; }
                if cy1 < min_y { min_y = cy1; }
                if cx2 > max_x { max_x = cx2; }
                if cy2 > max_y { max_y = cy2; }
            }

            // 2. Calcola l'offset standard (es. 20px in basso a destra dall'originale)
            let mut offset_x = 20.0;
            let mut offset_y = 20.0;

            // 3. Sistema Anti-Uscita dai Bordi (Clamping su PAGE_W e PAGE_H)
            if max_x + offset_x > crate::models::page::PAGE_W { offset_x = crate::models::page::PAGE_W - max_x; }
            if max_y + offset_y > crate::models::page::PAGE_H { offset_y = crate::models::page::PAGE_H - max_y; }
            if min_x + offset_x < 0.0 { offset_x = -min_x; }
            if min_y + offset_y < 0.0 { offset_y = -min_y; }

            if offset_x < 0.0 && max_x >= crate::models::page::PAGE_W { offset_x = 0.0; }
            if offset_y < 0.0 && max_y >= crate::models::page::PAGE_H { offset_y = 0.0; }

            // 4. Trasla gli elementi e prepara l'incollatura
            let mut new_elements: Vec<crate::models::page::ComponentPayload> = Vec::new();
            
            for comp in &mut st.clipboard {
                match comp {
                    crate::models::page::ComponentPayload::PenStroke(s) |
                    crate::models::page::ComponentPayload::EraserStroke(s) => {
                        for pt in &mut s.points {
                            pt.0 += offset_x;
                            pt.1 += offset_y;
                        }
                    }
                    crate::models::page::ComponentPayload::RichText(b) => {
                        b.x += offset_x;
                        b.y += offset_y;
                    }
                    crate::models::page::ComponentPayload::Image(b) => {
                        b.x += offset_x;
                        b.y += offset_y;
                    }
                    crate::models::page::ComponentPayload::Shape(s) => {
                        s.x1 += offset_x;
                        s.x2 += offset_x;
                        s.y1 += offset_y;
                        s.y2 += offset_y;
                    }
                }
                new_elements.push(comp.clone());
            }

            // 5. Inserimento VERO nel Database e nello stack
            // Usiamo commit_component che aggiunge il payload a components, lo inserisce
            // nel DB sqlite, e pusha il suo nuovo row_id dentro undo_stack!
            let start_idx = st.current_page_data.components.len();
            
            for comp in new_elements {
                st.commit_component(comp);
            }
            
            let end_idx = st.current_page_data.components.len();
            
            // 6. Aggiorna la selezione in modo che l'utente possa subito muovere gli elementi incollati
            st.selected_indices = (start_idx..end_idx).collect();
            if end_idx - start_idx == 1 {
                st.selected_index = Some(start_idx);
            } else {
                st.selected_index = None;
            }

            // Rimuove il "focus" su una singola selezione vecchia, forzando la visuale sui nuovi
            st.drag_mode = crate::models::select::DragMode::None;

            drop(st);
            canvas.queue_draw();

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