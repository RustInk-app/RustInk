use crate::models::page::*;

use crate::translate_xournal::*;

use crate::save_handler::autosave;
use crate::save_handler::db::*;
use crate::save_handler::database_pdf_utilities::*;
use crate::save_handler::database_utilities::*;

use crate::gui::sidebar::*;
use crate::gui::state::*;
use crate::gui::utils::*;

use glib::Propagation;
use glib::clone;
use gtk::prelude::*;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::save_handler::autosave::*;
use crate::save_handler::autosave_utilities::*;

use std::path::Path;

pub fn import_pdf_background(
    source_path: &Path,
    db_tmp_path: &Path, 
    start_order: i64,
) -> Result<(i64, usize, Option<(i64, usize)>, String), String> {
    
    
    let uri = gio::File::for_path(source_path).uri();
    let doc = poppler::Document::from_file(&uri, None)
        .map_err(|e| format!("invalid/corrupted PDF : {e}"))?;
    let n_pages = doc.n_pages();
    if n_pages <= 0 {
        return Err("The PDF doesn't contain any pages".into());
    }

    
    std::fs::create_dir_all(docs_dir()).map_err(|e| e.to_string())?;
    let uuid = uuid::Uuid::new_v4();
    let dest_filename = format!("{uuid}.pdf");
    let dest_path = docs_dir().join(&dest_filename);
    
    
    std::fs::copy(source_path, &dest_path).map_err(|e| e.to_string())?;

    let original_name = source_path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document.pdf".into());
    let relative_path = format!("docs/{dest_filename}");

    
    let mut conn = rusqlite::Connection::open(db_tmp_path).map_err(|e| e.to_string())?;

    let _ = conn.execute(
        "INSERT INTO pdf_documents (relative_path, original_name, page_count)
        VALUES (?1, ?2, ?3)",
        rusqlite::params![&relative_path, &original_name, n_pages as i64],
    );

    let doc_id = conn.last_insert_rowid();

    
    let first_id = insert_pdf_backed_pages_bulk(
        &mut conn,
        start_order,
        doc_id,
        n_pages as usize
    ).map_err(|e| e.to_string())?;

    let first_new_id = Some((first_id, start_order as usize));

    
    Ok((doc_id, n_pages as usize, first_new_id, dest_filename))
}


pub fn on_import_pdf_clicked(
    window: &gtk::Window,
    state: &Rc<RefCell<AppState>>,
    canvas: &gtk::DrawingArea,
    spin_page: &gtk::SpinButton,
    lbl_tot: &gtk::Label,
    page_listbox: &gtk::ListBox,
) {
    let dialog = gtk::FileChooserDialog::new(
        Some("Import PDF"),
        Some(window),
        gtk::FileChooserAction::Open,
    );
    dialog.add_buttons(&[
        ("Cancel", gtk::ResponseType::Cancel),
        ("Import", gtk::ResponseType::Accept),
    ]);

    let filter = gtk::FileFilter::new();
    filter.add_pattern("*.pdf");
    filter.set_name(Some("PDF Documents"));
    dialog.add_filter(filter);

    if dialog.run() == gtk::ResponseType::Accept {
        if let Some(path) = dialog.filename() {
            unsafe { dialog.destroy(); }
            
            let mut st = state.borrow_mut();
            
            if let Some(conn) = &st.db {
                let _ = conn.execute_batch("DELETE FROM component_rtree; DELETE FROM pages;");
            }
            
            st.page_count = 0;
            st.doc_generation += 1; 
            st.undo_stack.clear();
            st.redo_stack.clear();
            st.thumbnail_cache.borrow_mut().clear();
            
            let db_tmp_path = match st.db_tmp_path.clone() {
                Some(p) => p,
                None => return, 
            };
            
            let start_order = 0; 
            drop(st);

            
            let loading = crate::gui::utils::show_loading_dialog(window, "Importing PDF...");
            
            let (tx, rx) = std::sync::mpsc::channel();
            let path_clone = path.clone();

            
            std::thread::spawn(move || {
                let res = import_pdf_background(&path_clone, &db_tmp_path, start_order);
                let _ = tx.send(res);
            });

            let s_clone = state.clone();
            let c_clone = canvas.clone();
            let sp_clone = spin_page.clone();
            let lt_clone = lbl_tot.clone();
            let lb_clone = page_listbox.clone();
            let w_clone = window.clone();
            let loading_weak = loading.downgrade();

            
            glib::idle_add_local(move || {
                match rx.try_recv() {
                    Ok(Ok((doc_id, n_pages, first_new_id, dest_filename))) => {
                        if let Some(ld) = loading_weak.upgrade() { unsafe { ld.destroy(); } }
                        
                        let mut st = s_clone.borrow_mut();
                        st.page_count += n_pages;
                        st.is_modified = true;

                        
                        
                        let full_path = docs_dir().join(&dest_filename);
                        let uri = gio::File::for_path(&full_path).uri();
                        
                        
                        
                        if let Ok(doc) = poppler::Document::from_file(&uri, None) {
                            st.pdf_cache.borrow_mut().insert(doc_id, doc);
                        } else {
                            eprintln!("Error loading PDF #1.");
                        }

                        if let Some((_id, idx)) = first_new_id {
                            let _ = st.switch_to_page(idx);
                        }
                        
                        let title = st.window_title();
                        drop(st);
                        
                        w_clone.set_title(&title);
                        crate::gui::sidebar::refresh_sidebar(&s_clone, &lb_clone, &c_clone, &sp_clone, &lt_clone);
                        c_clone.queue_draw();
                        
                        glib::ControlFlow::Break
                    }
                    Ok(Err(e)) => {
                        if let Some(ld) = loading_weak.upgrade() { unsafe { ld.destroy(); } }
                        let alert = gtk::MessageDialog::new(
                            Some(&w_clone), gtk::DialogFlags::MODAL, gtk::MessageType::Error, gtk::ButtonsType::Ok, "Error importing PDF #2"
                        );
                        alert.set_secondary_text(Some(&e));
                        alert.run();
                        unsafe { alert.destroy(); }
                        glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(_) => {
                        if let Some(ld) = loading_weak.upgrade() { unsafe { ld.destroy(); } }
                        glib::ControlFlow::Break
                    }
                }
            });
            return;
        }
    }
    unsafe { dialog.destroy(); }
}

pub fn on_export_pdf_clicked(window: &gtk::Window, state: &Rc<RefCell<AppState>>) {
    let dialog = gtk::FileChooserDialog::new(
        Some("Export PDF"),
        Some(window),
        gtk::FileChooserAction::Save,
    );
    dialog.add_buttons(&[
        ("Cancel", gtk::ResponseType::Cancel),
        ("Export", gtk::ResponseType::Accept),
    ]);
    dialog.set_current_name("test_document.pdf");

    if dialog.run() == gtk::ResponseType::Accept {
        if let Some(path) = dialog.filename() {
            unsafe { dialog.destroy(); }

            let st = state.borrow();
            let db_tmp_path = match st.db_tmp_path.clone() {
                Some(p) => p,
                None => return,
            };
            
            if let Some(conn) = &st.db {
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            }
            drop(st);

            
            let loading_dialog = gtk::MessageDialog::new(
                Some(window), gtk::DialogFlags::MODAL,
                gtk::MessageType::Info, gtk::ButtonsType::None,
                "PDF exporting...",
            );
            loading_dialog.show_all();

            let (tx, rx) = std::sync::mpsc::channel();
            let path_clone = path.clone();

            std::thread::spawn(move || {
                crate::export::export_document_to_pdf(&db_tmp_path, &path_clone, tx);
            });

            let w_clone = window.clone();
            let loading_weak = loading_dialog.downgrade();

            
            glib::idle_add_local(move || {
                match rx.try_recv() {
                    Ok(Ok(Some((corrente, totale)))) => {
                        if let Some(ld) = loading_weak.upgrade() {
                            if corrente == totale {
                                ld.set_text(Some("PDF exported!"));
                            } else {
                                ld.set_text(Some(&format!("Exporting PDF...\n Loaded {} on {} pages", corrente, totale)));
                            }
                        }
                        glib::ControlFlow::Continue
                    }
                    Ok(Ok(None)) => {
                        
                        if let Some(ld) = loading_weak.upgrade() { unsafe { ld.destroy(); } }
                        let success = gtk::MessageDialog::new(
                            Some(&w_clone), gtk::DialogFlags::MODAL,
                            gtk::MessageType::Info, gtk::ButtonsType::Ok,
                            "Export completed!",
                        );
                        success.run();
                        unsafe { success.destroy(); }
                        glib::ControlFlow::Break
                    }
                    Ok(Err(e)) => {
                        
                        if let Some(ld) = loading_weak.upgrade() { unsafe { ld.destroy(); } }
                        let alert = gtk::MessageDialog::new(
                            Some(&w_clone), gtk::DialogFlags::MODAL,
                            gtk::MessageType::Error, gtk::ButtonsType::Ok,
                            "Error export PDF",
                        );
                        alert.set_secondary_text(Some(&e));
                        alert.run();
                        unsafe { alert.destroy(); }
                        glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(_) => {
                        
                        if let Some(ld) = loading_weak.upgrade() { unsafe { ld.destroy(); } }
                        glib::ControlFlow::Break
                    }
                }
            });
            return;
        }
    }
    unsafe { dialog.destroy(); }
}

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

    let accel_group = gtk::AccelGroup::new();
    window.add_accel_group(&accel_group);
    let (key, modifier) = gtk::accelerator_parse("<Primary>s"); 
    menu_save.add_accelerator("activate", &accel_group, key, modifier, gtk::AccelFlags::VISIBLE);

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

        let loading = show_loading_dialog(&window, "Saving in progress...");
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let path_clone = target_path.clone();

        std::thread::spawn(move || {
            let result = export_medias(&tmp_path, &path_clone).map_err(|e| e.to_string());
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
                        clear_old_sessions();
                        write_autosave_sentinel(Some(&target_path));
                    } else if let Err(e) = result {
                        eprintln!("Error saving #1: {e}");
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
            "You have unsaved changes. Do you want to save before proceeding??",
        );
        confirm.add_button("Cancel", gtk::ResponseType::Cancel);
        confirm.add_button("Don't save", gtk::ResponseType::No);
        confirm.add_button("Save", gtk::ResponseType::Yes);
        let resp = confirm.run();
        unsafe { confirm.destroy(); }

        match resp {
            gtk::ResponseType::Cancel => false,
            gtk::ResponseType::No => true,
            gtk::ResponseType::Yes => {
                let dialog = gtk::FileChooserDialog::new(Some("Save document"), Some(&window), gtk::FileChooserAction::Save);
                dialog.add_button("Cancel", gtk::ResponseType::Cancel);
                dialog.add_button("Save", gtk::ResponseType::Accept);
                dialog.set_do_overwrite_confirmation(true);
                if let Some(p) = state.borrow().bundle_path.clone() { dialog.set_filename(p); }
                else { dialog.set_current_name("document.rastin"); }
                let f = gtk::FileFilter::new(); f.set_name(Some("RASTIN (*.rastin)")); f.add_pattern("*.rastin"); dialog.add_filter(f);

                let res = dialog.run();
                let path_opt = dialog.filename();
                unsafe { dialog.destroy(); }

                if res == gtk::ResponseType::Accept {
                    if let Some(path) = path_opt {
                        if let Some(tmp) = state.borrow().db_tmp_path.clone() {
                            { let st = state.borrow(); if let Some(conn) = &st.db { let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);"); } }
                            if export_medias(&tmp, &path).is_ok() {
                                state.borrow_mut().bundle_path = Some(path.clone());
                                state.borrow_mut().is_modified = false;
                                write_autosave_sentinel(Some(&path));
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
        let dialog = gtk::FileChooserDialog::new(Some("Save with name"), Some(&window), gtk::FileChooserAction::Save);
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Save", gtk::ResponseType::Accept);
        dialog.set_do_overwrite_confirmation(true);
        if let Some(p) = state.borrow().bundle_path.clone() { dialog.set_filename(p); }
        else { dialog.set_current_name("document.rastin"); }
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
            eprintln!("Error creating new document: {}", e);
        }
        
        
        st.rebuild_bookmark_index();
        if let Some(cb) = &st.update_bookmark_ui {
            cb(false); 
        }
        

        let title = st.window_title();
        drop(st);

        window.set_title(&title);
        spin_page.set_range(1.0, 1.0);
        spin_page.set_value(1.0);
        lbl_tot.set_text("of 1");
        refresh_sidebar(&state, &page_listbox, &canvas, &spin_page, &lbl_tot);
        canvas.queue_draw();
    });

    menu_new.connect_activate(clone!(@strong do_new => move |_| do_new()));

    let do_open = clone!(@strong state, @strong window, @strong canvas, @strong spin_page, @strong lbl_tot, @strong page_listbox, @strong prompt_save_if_modified => move || {
        if !prompt_save_if_modified() { return; }

        
        
        let open_dialog = gtk::FileChooserNative::new(
            Some("Open document"),
            Some(&window),
            gtk::FileChooserAction::Open,
            Some("Open"),
            Some("Cancel"),
        );

        let f1 = gtk::FileFilter::new(); 
        f1.set_name(Some("RASTIN (*.rastin)")); 
        f1.add_pattern("*.rastin"); 
        open_dialog.add_filter(f1);

        let f2 = gtk::FileFilter::new(); 
        f2.set_name(Some("Xournal++ (*.xopp)")); 
        f2.add_pattern("*.xopp"); 
        open_dialog.add_filter(f2);

        let f3 = gtk::FileFilter::new(); 
        f3.set_name(Some("All supported")); 
        f3.add_pattern("*.rastin"); 
        f3.add_pattern("*.xopp"); 
        open_dialog.add_filter(f3);

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
                    &format!("The file is already opened in another window:\n{}\n\nClose the other window before opening a new one", chosen.display()),
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

        let tmp = temp_db_dir();
        let loading = show_loading_dialog(&window, "Opening document…");

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
                    conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;
                    init_schema(&conn).map_err(|e| e.to_string())?;

                    for (i, xopp_page) in xopp_pages.iter().enumerate() {
                        let bg_int = match xopp_page.background {
                            PaperBackground::Ruled => 0,
                            PaperBackground::Plain => 1,
                            PaperBackground::Grid => 2,
                        };
                        conn.execute(
                            "INSERT INTO pages (display_order, background) VALUES (?1, ?2)",
                            rusqlite::params![i as i64, bg_int],
                        ).map_err(|e| e.to_string())?;
                        let page_id = conn.last_insert_rowid();
                        let blob = encode_payload_list(&xopp_page.components);
                        conn.execute(
                            "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)",
                            rusqlite::params![page_id, blob],
                        ).map_err(|e| e.to_string())?;
                    }

                    let count    = page_count(&conn).map_err(|e| e.to_string())?;
                    let first_id = page_id_at(&conn, 0).map_err(|e| e.to_string())?;
                    let first_page = load_page(&conn, first_id).map_err(|e| e.to_string())?;
                    Ok(OpenResult { page_count: count, first_id, first_page, conn, bundle_path: None, tmp })
                } else {
                    import_medias(&chosen_clone, &tmp).map_err(|e| e.to_string())?;
                    let conn = rusqlite::Connection::open(&tmp).map_err(|e| e.to_string())?;

                    
                    
                    
                    conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;

                    
                    
                    
                    
                    init_schema(&conn).map_err(|e| e.to_string())?;

                    let count    = page_count(&conn).map_err(|e| e.to_string())?;
                    let first_id = page_id_at(&conn, 0).map_err(|e| e.to_string())?;
                    let first_page = load_page(&conn, first_id).map_err(|e| e.to_string())?;
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
                    write_autosave_sentinel(res.bundle_path.as_ref());
                    
                    let mut st = s_clone.borrow_mut();
                    st.page_count = res.page_count; 
                    st.current_page = 0; 
                    st.current_page_id = res.first_id; 
                    st.current_page_data = res.first_page; 
                    st.bundle_path = res.bundle_path; 
                    st.is_modified = false; 
                    st.db_tmp_path = Some(res.tmp); 
                    st.db = Some(res.conn); 
                    st.undo_stack.clear(); 
                    st.redo_stack.clear();

                    
                    
                    
                    
                    st.current_pdf_ref = st.db.as_ref()
                        .and_then(|conn| get_page_pdf_ref(conn, res.first_id).ok().flatten())
                        .map(|(doc_id, page_index)| PdfPageRef { doc_id, page_index });

                    st.pdf_cache.borrow_mut().clear();
                    st.pdf_surface_cache.borrow_mut().clear();
                    st.thumbnail_cache.borrow_mut().clear();
                    st.pending_thumbnails.borrow_mut().clear();
                    st.image_cache.borrow_mut().clear();

                    
                    
                    
                    
                    
                    
                    st.doc_generation += 1;
                    st.thumb_req_stack.lock().unwrap().clear();

                    if let Some(pref) = st.current_pdf_ref {
                        st.ensure_pdf_loaded(pref.doc_id);
                    }

                    
                    st.rebuild_bookmark_index();
                    
                    
                    let is_bk = st.current_page_data.is_bookmarked;
                    if let Some(cb) = &st.update_bookmark_ui {
                        cb(is_bk);
                    }
                    

                    if let Some(ref bp) = st.bundle_path.clone() {
                        if bp.extension().and_then(|e| e.to_str()) == Some("rastin") {
                            let _ = st.acquire_lock(bp);
                        }
                    }
                    let title = st.window_title(); let page_count = st.page_count; let current_page = st.current_page; drop(st);

                    sp_clone.set_range(1.0, page_count as f64);
                    sp_clone.set_value((current_page + 1) as f64);
                    lt_clone.set_text(&format!("of {}", page_count));
                    w_clone.set_title(&title);
                    refresh_sidebar(&s_clone, &lb_clone, &c_clone, &sp_clone, &lt_clone);
                    c_clone.queue_draw();
                    glib::ControlFlow::Break
                }
                Ok(Err(e)) => {
                    eprintln!("Error opening: {e}");
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

    if let Some(file_import_pdf) = builder.object::<gtk::MenuItem>("file_import_pdf") {
        let w = window.clone();
        let s = state.clone();
        let c = canvas.clone();
        let sp = spin_page.clone();
        let lt = lbl_tot.clone();
        let pl = page_listbox.clone();
        file_import_pdf.connect_activate(move |_| {
            on_import_pdf_clicked(&w, &s, &c, &sp, &lt, &pl);
        });
    }

    if let Some(file_export_pdf) = builder.object::<gtk::MenuItem>("file_export_pdf") {
        let w = window.clone();
        let s = state.clone();
        file_export_pdf.connect_activate(move |_| {
            on_export_pdf_clicked(&w, &s);
        });
    }
}

pub(crate) fn setup_autosave(state: &Rc<RefCell<AppState>>) {
    let s = state.clone();
    glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
        let (tmp_path, bundle_path) = {
            let st = s.borrow();
            (st.db_tmp_path.clone(), st.bundle_path.clone())
        };
        if let Some(tmp) = tmp_path {
            let dest = autosave_path();
            {
                let st = s.borrow();
                if let Some(conn) = &st.db {
                    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                }
            }
            std::thread::spawn(move || match export_medias(&tmp, &dest) {
                Ok(_) => eprintln!("Backup saved in {:?}", dest),
                Err(e) => eprintln!("Error auto-save: {e}"),
            });
            write_autosave_sentinel(bundle_path.as_ref());
            let mut backups: Vec<PathBuf> = std::fs::read_dir(autosave_path())
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
                "You have unsaved changes. Do you want to save before proceeding??",
            );
            dialog.add_button("Exit without saving", gtk::ResponseType::No);
            dialog.add_button("Cancel", gtk::ResponseType::Cancel);
            let btn_save = dialog.add_button("Save", gtk::ResponseType::Yes);
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
                        Some("Save document"),
                        Some(&w),
                        gtk::FileChooserAction::Save,
                    );
                    save_dialog.add_button("Cancel", gtk::ResponseType::Cancel);
                    save_dialog.add_button("Save", gtk::ResponseType::Accept);
                    save_dialog.set_do_overwrite_confirmation(true);
                    let filter = gtk::FileFilter::new();
                    filter.set_name(Some("RASTIN (*.rastin)"));
                    filter.add_pattern("*.rastin");
                    save_dialog.add_filter(filter);

                    if let Some(p) = s.borrow().bundle_path.clone() {
                        save_dialog.set_filename(p);
                    } else {
                        save_dialog.set_current_name("document.rastin");
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
                            match export_medias(&tmp, &path) {
                                Ok(_) => eprintln!("Saved in {:?}", path),
                                Err(e) => eprintln!("Error saving #5: {e}"),
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        s.borrow_mut().release_lock();

        if let Ok(entries) = std::fs::read_dir(backup_dir()) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }

        win.hide();
        Propagation::Proceed
    });
}
