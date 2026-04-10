use crate::models::page::*;
use crate::models::stroke::*;

use crate::translate_xournal::*;

use crate::save_handler::autosave;
use crate::save_handler::db::*;

use crate::gui::canvas_events::*;
use crate::gui::drawing::*;
use crate::gui::shortcuts::*;
use crate::gui::sidebar::*;
use crate::gui::state::*;
use crate::gui::toolbar::*;
use crate::gui::utils::*;

use glib::Propagation;
use glib::clone;
use gtk::prelude::*;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) fn setup_file_ops(
    builder: &gtk::Builder,
    window: &gtk::Window,
    canvas: &gtk::DrawingArea,
    state: &Rc<RefCell<AppState>>,
    spin_page: &gtk::SpinButton,
    lbl_tot: &gtk::Label,
    page_listbox: &gtk::ListBox,
    btn_save: &gtk::Button,
    btn_open: &gtk::Button,
) {
    let menu_new: gtk::MenuItem = builder.object("file_new").unwrap();
    let menu_open: gtk::MenuItem = builder.object("file_open").unwrap();
    let menu_save: gtk::MenuItem = builder.object("file_save").unwrap();
    let menu_save_as: gtk::MenuItem = builder.object("file_save_as").unwrap();

    let execute_save_background = clone!(@strong state, @strong window => move |target_path: PathBuf| {
        let tmp_path = match state.borrow().db_tmp_path.clone() {
            Some(t) => t,
            None => return,
        };

        {
            let st = state.borrow();
            if let Some(conn) = &st.db {
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            }
        }

        let loading = show_loading_dialog(&window, "Salvataggio in corso…");
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let path_clone = target_path.clone();

        std::thread::spawn(move || {
            let result = export_bundle_path(&tmp_path, &path_clone).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });

        let s_clone = state.clone();
        let w_clone = window.clone();
        let loading_weak = loading.downgrade();

        glib::idle_add_local(move || {
            match rx.try_recv() {
                Ok(result) => {
                    if let Some(ld) = loading_weak.upgrade() { unsafe { ld.destroy(); } }
                    if result.is_ok() {
                        let mut st = s_clone.borrow_mut();

                        st.bundle_path = Some(target_path.clone());
                        st.is_modified = false;
                        st.release_lock();
                        let _ = st.acquire_lock(&target_path);
                        let title = st.window_title();
                        drop(st);
                        w_clone.set_title(&title);
                        autosave::clear_old_sessions();
                        autosave::write_autosave_sentinel(Some(&target_path));
                    } else if let Err(e) = result {
                        eprintln!("[DB] ERRORE salvataggio: {e}");
                    }
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(_) => {
                    if let Some(ld) = loading_weak.upgrade() { unsafe { ld.destroy(); } }
                    glib::ControlFlow::Break
                }
            }
        });
    });

    let prompt_save_if_modified = clone!(@strong state, @strong window => move || {
        if !state.borrow().is_modified { return true; }

        let confirm = gtk::MessageDialog::new(
            Some(&window), gtk::DialogFlags::MODAL, gtk::MessageType::Question,
            gtk::ButtonsType::None,
            "Hai modifiche non salvate. Vuoi salvare prima di procedere?",
        );
        confirm.add_button("Annulla", gtk::ResponseType::Cancel);
        confirm.add_button("Non salvare", gtk::ResponseType::No);
        confirm.add_button("Salva", gtk::ResponseType::Yes);
        let resp = confirm.run();
        unsafe { confirm.destroy(); }

        match resp {
            gtk::ResponseType::Cancel => false,
            gtk::ResponseType::No => true,
            gtk::ResponseType::Yes => {
                let dialog = gtk::FileChooserDialog::new(Some("Salva documento"), Some(&window), gtk::FileChooserAction::Save);
                dialog.add_button("Annulla", gtk::ResponseType::Cancel);
                dialog.add_button("Salva", gtk::ResponseType::Accept);
                dialog.set_do_overwrite_confirmation(true);
                if let Some(p) = state.borrow().bundle_path.clone() { dialog.set_filename(p); }
                else { dialog.set_current_name("documento.rastin"); }
                let f = gtk::FileFilter::new(); f.set_name(Some("RASTIN (*.rastin)")); f.add_pattern("*.rastin"); dialog.add_filter(f);

                let res = dialog.run();
                let path_opt = dialog.filename();
                unsafe { dialog.destroy(); }

                if res == gtk::ResponseType::Accept {
                    if let Some(path) = path_opt {
                        if let Some(tmp) = state.borrow().db_tmp_path.clone() {
                            { let st = state.borrow(); if let Some(conn) = &st.db { let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);"); } }
                            if export_bundle_path(&tmp, &path).is_ok() {
                                state.borrow_mut().bundle_path = Some(path.clone());
                                state.borrow_mut().is_modified = false;
                                autosave::write_autosave_sentinel(Some(&path));
                                return true;
                            }
                        }
                    }
                }
                false
            }
            _ => false,
        }
    });

    let do_save_as = clone!(@strong state, @strong window, @strong execute_save_background => move || {
        let dialog = gtk::FileChooserDialog::new(Some("Salva con nome"), Some(&window), gtk::FileChooserAction::Save);
        dialog.add_button("Annulla", gtk::ResponseType::Cancel);
        dialog.add_button("Salva", gtk::ResponseType::Accept);
        dialog.set_do_overwrite_confirmation(true);
        if let Some(p) = state.borrow().bundle_path.clone() { dialog.set_filename(p); }
        else { dialog.set_current_name("documento.rastin"); }
        let f = gtk::FileFilter::new(); f.set_name(Some("RASTIN (*.rastin)")); f.add_pattern("*.rastin"); dialog.add_filter(f);

        let res = dialog.run();
        let path_opt = dialog.filename();
        unsafe { dialog.destroy(); }

        if res == gtk::ResponseType::Accept {
            execute_save_background(path_opt.unwrap());
        }
    });

    menu_save_as.connect_activate(clone!(@strong do_save_as => move |_| do_save_as()));

    let do_save = clone!(@strong state, @strong execute_save_background, @strong do_save_as => move || {
        let path_opt = state.borrow().bundle_path.clone();
        if let Some(path) = path_opt {

            execute_save_background(path);
        } else {

            do_save_as();
        }
    });

    btn_save.connect_clicked(clone!(@strong do_save => move |_| do_save()));
    menu_save.connect_activate(clone!(@strong do_save => move |_| do_save()));

    let do_new = clone!(@strong state, @strong window, @strong canvas, @strong spin_page, @strong lbl_tot, @strong page_listbox, @strong prompt_save_if_modified => move || {
        if !prompt_save_if_modified() { return; }

        let mut st = state.borrow_mut();
        st.db = None;
        if let Some(old_tmp) = st.db_tmp_path.take() {
            let _ = std::fs::remove_file(&old_tmp);
            let _ = std::fs::remove_file(old_tmp.with_extension("sqlite-wal"));
            let _ = std::fs::remove_file(old_tmp.with_extension("sqlite-shm"));
        }

        if let Err(e) = st.init_new_document() {
            eprintln!("Errore creazione nuovo doc: {}", e);
        }
        let title = st.window_title();
        drop(st);

        window.set_title(&title);
        spin_page.set_range(1.0, 1.0);
        spin_page.set_value(1.0);
        lbl_tot.set_text("di 1");
        refresh_sidebar(&state, &page_listbox, &canvas, &spin_page, &lbl_tot);
        canvas.queue_draw();
    });

    menu_new.connect_activate(clone!(@strong do_new => move |_| do_new()));

    let do_open = clone!(@strong state, @strong window, @strong canvas, @strong spin_page, @strong lbl_tot, @strong page_listbox, @strong prompt_save_if_modified => move || {
        if !prompt_save_if_modified() { return; }

        let open_dialog = gtk::FileChooserDialog::new(Some("Apri documento"), Some(&window), gtk::FileChooserAction::Open);
        open_dialog.add_button("Annulla", gtk::ResponseType::Cancel); open_dialog.add_button("Apri", gtk::ResponseType::Accept);
        let f1 = gtk::FileFilter::new(); f1.set_name(Some("RASTIN (*.rastin)")); f1.add_pattern("*.rastin"); open_dialog.add_filter(f1);
        let f2 = gtk::FileFilter::new(); f2.set_name(Some("Xournal++ (*.xopp)")); f2.add_pattern("*.xopp"); open_dialog.add_filter(f2);
        let f3 = gtk::FileFilter::new(); f3.set_name(Some("Tutti i supportati")); f3.add_pattern("*.rastin"); f3.add_pattern("*.xopp"); open_dialog.add_filter(f3);

        let accepted = open_dialog.run() == gtk::ResponseType::Accept;
        let chosen   = open_dialog.filename();
        unsafe { open_dialog.destroy(); }
        if !accepted { return; }
        let chosen = match chosen { Some(p) => p, None => return };

        let ext = chosen.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();


        if ext == "rastin" {
            let lock_file = chosen.with_extension("rastin.lock");
            if lock_file.exists() {
                let dlg = gtk::MessageDialog::new(
                    Some(&window), gtk::DialogFlags::MODAL, gtk::MessageType::Error, gtk::ButtonsType::Ok,
                    &format!("Il file è già aperto in un'altra istanza:\n{}\n\nChiudi l'altra istanza prima di aprirlo.", chosen.display()),
                );
                dlg.run(); unsafe { dlg.destroy(); }
                return;
            }
        }


        {
            let mut st = state.borrow_mut();
            st.db = None;
            if let Some(old_tmp) = st.db_tmp_path.take() {
                let _ = std::fs::remove_file(&old_tmp);
                let _ = std::fs::remove_file(old_tmp.with_extension("sqlite-wal"));
                let _ = std::fs::remove_file(old_tmp.with_extension("sqlite-shm"));
            }
            st.undo_stack.clear();
            st.redo_stack.clear();
        }

        let tmp = autosave::temp_db_dir();
        let loading = show_loading_dialog(&window, "Apertura documento in corso…");

        pub struct OpenResult {
            page_count: usize, first_id: i64, first_page: PageData, conn: rusqlite::Connection, bundle_path: Option<PathBuf>, tmp: PathBuf,
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<OpenResult, String>>();
        let chosen_clone = chosen.clone();
        let ext_clone    = ext.clone();

        std::thread::spawn(move || {
            let result = (|| -> Result<OpenResult, String> {
                if ext_clone == "xopp" {
                    let xopp_pages = import_xopp(&chosen_clone).map_err(|e| e.to_string())?;
                    let _ = std::fs::remove_file(&tmp);
                    let conn = rusqlite::Connection::open(&tmp).map_err(|e| e.to_string())?;
                    init_schema(&conn).map_err(|e| e.to_string())?;

                    for (i, xopp_page) in xopp_pages.iter().enumerate() {
                        conn.execute("INSERT INTO pages (display_order) VALUES (?1)", rusqlite::params![i as i64]).map_err(|e| e.to_string())?;
                        let page_id = conn.last_insert_rowid();
                        let components: Vec<ComponentPayload> = xopp_page.strokes.iter().map(|s: &Stroke| ComponentPayload::PenStroke(s.clone())).collect();
                        let blob = encode_payload_list(&components);
                        conn.execute("INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)", rusqlite::params![page_id, blob]).map_err(|e| e.to_string())?;
                    }

                    let count    = page_count(&conn).unwrap_or(1);
                    let first_id = page_id_at(&conn, 0).unwrap_or(1);
                    let first_page = load_page(&conn, first_id).unwrap_or_default();
                    Ok(OpenResult { page_count: count, first_id, first_page, conn, bundle_path: None, tmp })
                } else {
                    import_bundle(&chosen_clone, &tmp).map_err(|e| e.to_string())?;
                    let conn = rusqlite::Connection::open(&tmp).map_err(|e| e.to_string())?;
                    let count    = page_count(&conn).unwrap_or(1);
                    let first_id = page_id_at(&conn, 0).unwrap_or(1);
                    let first_page = load_page(&conn, first_id).unwrap_or_default();
                    Ok(OpenResult { page_count: count, first_id, first_page, conn, bundle_path: Some(chosen_clone), tmp })
                }
            })();
            let _ = tx.send(result);
        });

        let s_clone  = state.clone(); let sp_clone = spin_page.clone(); let lt_clone = lbl_tot.clone(); let lb_clone = page_listbox.clone(); let c_clone  = canvas.clone(); let w_clone  = window.clone(); let lw = loading.downgrade();

        glib::idle_add_local(move || {
            match rx.try_recv() {
                Ok(Ok(res)) => {
                    if let Some(ld) = lw.upgrade() { unsafe { ld.destroy(); } }
                    autosave::write_autosave_sentinel(res.bundle_path.as_ref());
                    let mut st = s_clone.borrow_mut();
                    st.page_count = res.page_count; st.current_page = 0; st.current_page_id = res.first_id; st.current_page_data = res.first_page; st.bundle_path = res.bundle_path; st.is_modified = false; st.db_tmp_path = Some(res.tmp); st.db = Some(res.conn); st.undo_stack.clear(); st.redo_stack.clear();

                    if let Some(ref bp) = st.bundle_path.clone() {
                        if bp.extension().and_then(|e| e.to_str()) == Some("rastin") {
                            let _ = st.acquire_lock(bp);
                        }
                    }
                    let title = st.window_title(); let page_count = st.page_count; let current_page = st.current_page; drop(st);

                    sp_clone.set_range(1.0, page_count as f64);
                    sp_clone.set_value((current_page + 1) as f64);
                    lt_clone.set_text(&format!("di {}", page_count));
                    w_clone.set_title(&title);
                    refresh_sidebar(&s_clone, &lb_clone, &c_clone, &sp_clone, &lt_clone);
                    c_clone.queue_draw();
                    glib::ControlFlow::Break
                }
                Ok(Err(e)) => {
                    eprintln!("[DB] Errore apertura: {e}");
                    if let Some(ld) = lw.upgrade() { unsafe { ld.destroy(); } }
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(_) => {
                    if let Some(ld) = lw.upgrade() { unsafe { ld.destroy(); } }
                    glib::ControlFlow::Break
                }
            }
        });
    });

    btn_open.connect_clicked(clone!(@strong do_open => move |_| do_open()));
    menu_open.connect_activate(clone!(@strong do_open => move |_| do_open()));
}

pub(crate) fn setup_autosave(state: &Rc<RefCell<AppState>>) {
    let s = state.clone();
    glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
        let (tmp_path, bundle_path) = {
            let st = s.borrow();
            (st.db_tmp_path.clone(), st.bundle_path.clone())
        };
        if let Some(tmp) = tmp_path {
            let dest = autosave::autosave_path();
            {
                let st = s.borrow();
                if let Some(conn) = &st.db {
                    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                }
            }
            std::thread::spawn(move || match export_bundle_path(&tmp, &dest) {
                Ok(_) => eprintln!("[AUTOSAVE] Backup salvato in {:?}", dest),
                Err(e) => eprintln!("[AUTOSAVE] Errore: {e}"),
            });
            autosave::write_autosave_sentinel(bundle_path.as_ref());
            let mut backups: Vec<PathBuf> = std::fs::read_dir(autosave::autosave_path())
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("rastin"))
                .collect();
            backups.sort();
            while backups.len() > 5 {
                let _ = std::fs::remove_file(backups.remove(0));
            }
        }
        glib::ControlFlow::Continue
    });
}

pub(crate) fn setup_window_close(window: &gtk::Window, state: &Rc<RefCell<AppState>>) {
    let s = state.clone();
    let w = window.clone();
    window.connect_delete_event(move |win, _| {
        let is_modified = s.borrow().is_modified;
        if is_modified {
            let dialog = gtk::MessageDialog::new(
                Some(&w),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Warning,
                gtk::ButtonsType::None,
                "Hai modifiche non salvate. Vuoi salvare prima di uscire?",
            );
            dialog.add_button("Esci senza salvare", gtk::ResponseType::No);
            dialog.add_button("Annulla", gtk::ResponseType::Cancel);
            let btn_save = dialog.add_button("Salva", gtk::ResponseType::Yes);
            btn_save.style_context().add_class("suggested-action");
            dialog.set_default_response(gtk::ResponseType::Yes);

            let resp = dialog.run();
            unsafe {
                dialog.destroy();
            }

            match resp {
                gtk::ResponseType::Cancel => return Propagation::Stop,
                gtk::ResponseType::Yes => {
                    let save_dialog = gtk::FileChooserDialog::new(
                        Some("Salva documento"),
                        Some(&w),
                        gtk::FileChooserAction::Save,
                    );
                    save_dialog.add_button("Annulla", gtk::ResponseType::Cancel);
                    save_dialog.add_button("Salva", gtk::ResponseType::Accept);
                    save_dialog.set_do_overwrite_confirmation(true);
                    let filter = gtk::FileFilter::new();
                    filter.set_name(Some("RASTIN (*.rastin)"));
                    filter.add_pattern("*.rastin");
                    save_dialog.add_filter(filter);

                    if let Some(p) = s.borrow().bundle_path.clone() {
                        save_dialog.set_filename(p);
                    } else {
                        save_dialog.set_current_name("documento.rastin");
                    }

                    let save_resp = save_dialog.run();
                    let save_path = save_dialog.filename();
                    unsafe {
                        save_dialog.destroy();
                    }

                    if save_resp != gtk::ResponseType::Accept {
                        return Propagation::Stop;
                    }

                    if let Some(path) = save_path {
                        let tmp_opt = s.borrow().db_tmp_path.clone();
                        if let Some(tmp) = tmp_opt {
                            {
                                let st = s.borrow();
                                if let Some(conn) = &st.db {
                                    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                                }
                            }
                            match export_bundle_path(&tmp, &path) {
                                Ok(_) => eprintln!("[CHIUSURA] Salvato in {:?}", path),
                                Err(e) => eprintln!("[CHIUSURA] ERRORE salvataggio: {e}"),
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        s.borrow_mut().release_lock();
        win.hide();
        Propagation::Proceed
    });
}
