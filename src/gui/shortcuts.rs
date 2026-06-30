

use crate::models::page::*;
use crate::models::select::*;
use crate::save_handler::db::encode_payload_list;

use gtk::prelude::*;
use gtk::gdk;
use glib::Propagation;

use std::cell::RefCell;
use std::rc::Rc;
use crate::gui::clone;
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
        // ... (IL RESTO DEGLI SHORTCUT INVARIATI FINO A:)
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

    // AGGIUNGI QUESTO BLOCCO PER INTERCETTARE IL RILASCIO DEL TASTO E RIPRISTINARE IL TOOL
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
}