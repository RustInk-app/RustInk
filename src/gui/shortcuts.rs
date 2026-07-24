use crate::models::page::*;
use crate::models::select::*;
use crate::save_handler::db::*;
use crate::save_handler::database_pdf_utilities::*;
use crate::save_handler::database_utilities::*;

use gtk::prelude::*;
use gtk::gdk;
use glib::Propagation;
use glib::clone;

use std::cell::RefCell;
use std::rc::Rc;

use crate::gui::state::*;
use crate::models::image::*;
use crate::save_handler::autosave::*;
use crate::save_handler::autosave_utilities::*;


use std::cell::Cell;

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

    // Flag creato UNA SOLA VOLTA, fuori dalla closure del tasto: rappresenta
    // "l'ultima operazione clipboard è stata un Ctrl+C interno?". Se ricreato
    // dentro la closure (come nella versione precedente) tornerebbe sempre a
    // `false` ad ogni pressione di tasto, perdendo lo stato.
    let internal_copy_active = Rc::new(Cell::new(false));

    // Collegato una sola volta: quando la clipboard di SISTEMA cambia
    // davvero (screenshot, copia da browser, da un altro programma...),
    // il flag torna false, così Ctrl+V riprende a guardare l'esterno.
    // Usiamo `connect_local` (generico, via glib::ObjectExt) al posto di
    // `connect_owner_change`, che non è generato come binding sicuro per
    // questo segnale in questa versione dei binding GTK.
    {
        let flag = internal_copy_active.clone();
        let gtk_clipboard = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD);
        gtk_clipboard.connect_local("owner-change", false, move |_| {
            flag.set(false);
            None
        });
    }

    window.connect_key_press_event(clone!(@strong s as state, @strong c as canvas, @strong w as window, @strong internal_copy_active as internal_copy_active => move |_, event| {
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

            // Segnaliamo che l'ultima copia valida è quella INTERNA:
            // il prossimo Ctrl+V deve ignorare la clipboard di sistema
            // finché non arriva una vera copia esterna (owner-change).
            internal_copy_active.set(true);

            return Propagation::Stop;
        }

        if ctrl && (key == keys::v || key == keys::V) {
            // --- CTRL + V : INCOLLA (elementi interni + fonti esterne, unificato) ---

            // 1. Controlliamo la clipboard di SISTEMA solo se NON è stato appena
            //    fatto un Ctrl+C interno. Se il flag è true, saltiamo del tutto
            //    questo blocco e incolliamo direttamente gli elementi già in
            //    st.clipboard, evitando che un'immagine esterna "vecchia" (mai
            //    aggiornata dall'ultima volta) sovrascriva la copia interna.
            if !internal_copy_active.get() {
                let gtk_clipboard = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD);

                if gtk_clipboard.wait_is_image_available() {
                    if let Some(pixbuf) = gtk_clipboard.wait_for_image() {
                        let raw_bytes = match pixbuf_to_raw_bytes(&pixbuf) {
                            Some(b) => b,
                            None => {
                                show_format_error_dialog(&window);
                                return Propagation::Stop;
                            }
                        };

                        if detect_image_format(&raw_bytes).is_none() {
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

                        let max_w = crate::models::page::PAGE_W * 0.90;
                        let max_h = crate::models::page::PAGE_H * 0.90;
                        let scale = (max_w / iw_orig).min(max_h / ih_orig).min(1.0);
                        let iw = iw_orig * scale;
                        let ih = ih_orig * scale;

                        // --- NUOVO: RECUPERO DEL CENTRO DINAMICO DELLA VIEWPORT ---
                        let mut hadj = None;
                        let mut vadj = None;
                        let mut current_parent = canvas.parent();
                        while let Some(widget) = current_parent {
                            if let Ok(sw) = widget.clone().downcast::<gtk::ScrolledWindow>() {
                                hadj = Some(sw.hadjustment());
                                vadj = Some(sw.vadjustment());
                                break;
                            }
                            current_parent = widget.parent();
                        }

                        // Calcola il centro dello schermo visibile convertendolo in coordinate del foglio
                        let (px_center, py_center) = if let (Some(h), Some(v)) = (hadj, vadj) {
                            let cx = h.value() + h.page_size() / 2.0;
                            let cy = v.value() + v.page_size() / 2.0;
                            let (ox, oy) = state.borrow().page_origin;
                            let zoom = state.borrow().zoom;
                            ((cx - ox) / zoom, (cy - oy) / zoom)
                        } else {
                            (crate::models::page::PAGE_W / 2.0, crate::models::page::PAGE_H / 2.0)
                        };

                        let mut x = px_center - iw / 2.0;
                        let mut y = py_center - ih / 2.0;

                        // --- GESTIONE DEI BORDI (Sopra/Sotto/Lati se manca spazio) ---
                        if x + iw > crate::models::page::PAGE_W { x = crate::models::page::PAGE_W - iw; }
                        if x < 0.0 { x = 0.0; }
                        if y + ih > crate::models::page::PAGE_H { y = crate::models::page::PAGE_H - ih; }
                        if y < 0.0 { y = 0.0; }

                        let block = crate::models::image::ImageBlock {
                            filename: bundle_entry,
                            x,
                            y,
                            width:  iw,
                            height: ih,
                        };

                        // Qui trasferiamo il contenuto "esterno" dentro la clipboard interna
                        state.borrow_mut().clipboard =
                            vec![crate::models::page::ComponentPayload::Image(block)];
                    }
                }
            }

            // 2. Da qui in poi la logica lavora su st.clipboard, sia che provenga
            //    da un Ctrl+C interno sia da fonti esterne.
            let mut st = state.borrow_mut();
            if st.clipboard.is_empty() {
                return Propagation::Proceed;
            }

            // 2a. Calcola l'ingombro massimo e minimo degli elementi nella clipboard
            let mut min_x = f64::MAX;
            let mut min_y = f64::MAX;
            let mut max_x = f64::MIN;
            let mut max_y = f64::MIN;

            for comp in &st.clipboard {
                let (cx1, cx2, cy1, cy2) = bounding_box(comp);
                if cx1 < min_x { min_x = cx1; }
                if cy1 < min_y { min_y = cy1; }
                if cx2 > max_x { max_x = cx2; }
                if cy2 > max_y { max_y = cy2; }
            }

            // 2b. Calcola l'offset standard: applichiamo +20px SOLO per copie interne,
            // così l'immagine da clipboard esterna non subisce spostamenti indesiderati.
            let mut offset_x = if internal_copy_active.get() { 20.0 } else { 0.0 };
            let mut offset_y = if internal_copy_active.get() { 20.0 } else { 0.0 };

            // 2c. Anti-uscita dai bordi finale (ulteriore livello di sicurezza)
            if max_x + offset_x > crate::models::page::PAGE_W { offset_x = crate::models::page::PAGE_W - max_x; }
            if max_y + offset_y > crate::models::page::PAGE_H { offset_y = crate::models::page::PAGE_H - max_y; }
            if min_x + offset_x < 0.0 { offset_x = -min_x; }
            if min_y + offset_y < 0.0 { offset_y = -min_y; }

            if offset_x < 0.0 && max_x >= crate::models::page::PAGE_W { offset_x = 0.0; }
            if offset_y < 0.0 && max_y >= crate::models::page::PAGE_H { offset_y = 0.0; }

            // 2d. Trasla gli elementi
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

            // 2e. Inserimento nel DB e nello stack degli snapshot (Undo funzionante)
            let start_idx = st.current_page_data.components.len();
            
            st.save_snapshot();

            for comp in new_elements {
                if let Some(conn) = &st.db {
                    let _ = append_active_component(conn, st.current_page_id, &comp);
                }
                st.current_page_data.components.push(comp);
            }

            let end_idx = st.current_page_data.components.len();
            st.thumbnail_cache.borrow_mut().remove(&st.current_page);
            st.is_modified = true;

            // 2f. Aggiorna la selezione sul nuovo elemento incollato
            st.selected_indices = (start_idx..end_idx).collect();
            if end_idx - start_idx == 1 {
                st.selected_index = Some(start_idx);
            } else {
                st.selected_index = None;
            }

            st.drag_mode = crate::models::select::DragMode::None;

            drop(st);
            canvas.queue_draw();

            let title = state.borrow().window_title();
            window.set_title(&title);

            return Propagation::Stop;
        }


        if key == Key::Delete || key == Key::BackSpace {
            let has_selection = !state.borrow().selected_indices.is_empty();
            if has_selection {
                let mut st = state.borrow_mut();
                st.save_snapshot();

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
                // st.undo_stack.clear();
                // st.redo_stack.clear();
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