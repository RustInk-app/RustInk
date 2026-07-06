use crate::save_handler::db::*;
use crate::gui::state::*;

use gtk::prelude::*;
use gtk::gdk;
use glib::Propagation;
use std::cell::RefCell;
use std::rc::Rc;

lazy_static::lazy_static! {
    pub(crate) static ref DND_TARGETS: [gtk::TargetEntry; 1] = [
        gtk::TargetEntry::new("application/x-rastin-page", gtk::TargetFlags::SAME_APP, 0),
    ];
}

pub(crate) fn setup_sidebar(builder: &gtk::Builder, state: &Rc<RefCell<AppState>>) -> gtk::ListBox {
    let sidebar_scrolled: gtk::ScrolledWindow = builder.object("first_panel_sidebar").expect("first_panel_sidebar non trovata");
    let sidebar_container: gtk::Box = builder.object("sidebar_container").expect("sidebar_container non trovata");
    
    for child in sidebar_scrolled.children() { sidebar_scrolled.remove(&child); }

    let page_listbox = gtk::ListBox::new();
    let css_provider = gtk::CssProvider::new();
    let dark_css = b"
        list { background-color: #2e2e34; color: white; }
        row { background-color: #2e2e34; }  
        row:hover { background-color: #3e3e44; } 
        row:selected { background-color: #4a90d9; color: white; } 
    ";
    if let Err(e) = css_provider.load_from_data(dark_css) {
        eprintln!("Errore nel caricamento del CSS della sidebar: {}", e);
    }
    page_listbox.style_context().add_provider(&css_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    sidebar_scrolled.add(&page_listbox);

    // Creiamo la SearchBar per i segnalibri
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Cerca nei segnalibri..."));
    search_entry.set_margin_start(8);
    search_entry.set_margin_end(8);
    search_entry.set_margin_bottom(8);
    
    // Inseriamo la searchbar sotto lo ScrolledWindow
    sidebar_container.pack_end(&search_entry, false, false, 0);
    sidebar_container.reorder_child(&search_entry, -1);

    // LOGICA DI RICERCA TRAMITE TRIE
    let s_search = state.clone();
    let lb_search = page_listbox.clone();
    search_entry.connect_search_changed(move |entry| {
        let query = entry.text().to_string().to_lowercase();
        s_search.borrow_mut().search_query = query;
        lb_search.invalidate_filter(); // Forza il re-rendering basato sulla funzione filtro
    });

    let s_filter = state.clone();
    page_listbox.set_filter_func(Some(Box::new(move |row: &gtk::ListBoxRow| {
        let st = s_filter.borrow();
        let idx = row.index() as usize;

        if st.search_query.is_empty() {
            return true; // Se la barra è vuota, mostriamo sia "Pagina N" che i Segnalibri
        }

        // Se stai cercando qualcosa, mostra SOLO le pagine salvate nei preferiti...
        if !st.bookmarked_pages.contains(&idx) {
            return false;
        }

        // ...che corrispondono a ciò che hai scritto
        let terms: Vec<&str> = st.search_query.split_whitespace().collect();
        for term in terms {
            if let Some(pages) = st.bookmark_trie.search(term) {
                if !pages.contains(&idx) {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    })));

    // --- START THREAD BACKGROUND PER LE MINIATURE (PRIORITA' LIFO) ---
    let req_stack = state.borrow().thumb_req_stack.clone();
    let (tx_wake, rx_wake) = std::sync::mpsc::channel::<()>();
    let (tx_res, rx_res) = glib::MainContext::channel(glib::Priority::DEFAULT);

    state.borrow_mut().thumb_wakeup_tx = Some(tx_wake);

    std::thread::spawn(move || {
        let mut current_db_key: Option<(std::path::PathBuf, u64)> = None;
        let mut conn: Option<rusqlite::Connection> = None;
        
        // Mantiene il PDF in RAM separatamente per il worker, risolvendo i crash e le righe!
        let mut poppler_cache = std::collections::HashMap::<i64, poppler::Document>::new();
        let pdf_surface_cache = std::cell::RefCell::new(std::collections::HashMap::new());

        // Il thread dorme finché non riceve un input
        while rx_wake.recv().is_ok() {
            loop {
                // Preleva SEMPRE l'ULTIMA miniatura richiesta (LIFO = quelle appena scrollate!)
                let req = {
                    let mut stack = req_stack.lock().unwrap();
                    stack.pop()
                };

                match req {
                    Some((db_path, page_index, generation)) => {
                        let key = (db_path.clone(), generation);
                        if current_db_key != Some(key.clone()) {
                            // Documento cambiato (anche se il path fisico è identico, com'è
                            // sempre il caso per il file di sessione): chiudiamo la vecchia
                            // connessione e ne apriamo una nuova, altrimenti restiamo agganciati
                            // al file precedente e blocchiamo/leggiamo dati vecchi quando il
                            // programma sovrascrive struttura.sqlite con un documento appena aperto.
                            conn = None;
                            current_db_key = Some(key);
                            conn = rusqlite::Connection::open_with_flags(
                                &db_path,
                                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI
                            ).ok();
                            if let Some(c) = &conn {
                                let _ = c.busy_timeout(std::time::Duration::from_secs(3));
                            }
                            poppler_cache.clear();
                            pdf_surface_cache.borrow_mut().clear();
                        }

                        if let Some(c) = &conn {
                            if let Ok(page_id) = crate::save_handler::db::page_id_at(c, page_index) {
                                if let Ok(page_data) = crate::save_handler::db::load_page(c, page_id) {
                                    
                                    let pdf_bg_ref = if let Ok(Some((doc_id, pdf_idx))) = crate::save_handler::db::get_page_pdf_ref(c, page_id) {
                                        // Usa la cache locale per non chiamare mai più from_file due volte!
                                        let doc = poppler_cache.entry(doc_id).or_insert_with(|| {
                                            let row = crate::save_handler::db::get_pdf_document(c, doc_id).unwrap();
                                            let full_path = crate::save_handler::autosave::SESSION_TEMP_DIR.path().join(&row.relative_path);
                                            let uri = gio::File::for_path(&full_path).uri();
                                            poppler::Document::from_file(&uri, None).unwrap()
                                        });
                                        Some((&*doc, doc_id, pdf_idx))
                                    } else {
                                        None
                                    };

                                    let mut surf = cairo::ImageSurface::create(cairo::Format::ARgb32, 100, 140).unwrap();
                                    let cr = cairo::Context::new(&surf).unwrap();
                                    cr.set_source_rgb(1.0, 1.0, 1.0);
                                    cr.paint().unwrap();
                                    
                                    let zoom = 100.0 / crate::models::page::PAGE_W;
                                    cr.scale(zoom, zoom);
                                    
                                    let empty_image_cache = std::cell::RefCell::new(std::collections::HashMap::new());
                                    
                                    crate::gui::drawing::draw_page(
                                        &cr, &page_data, 0.0, 0.0, zoom, &empty_image_cache, 
                                        &page_data.background, pdf_bg_ref, &pdf_surface_cache
                                    );
                                    
                                    drop(cr);
                                    let pixels = surf.data().unwrap().to_vec();
                                    let _ = tx_res.send((page_index, pixels));
                                }
                            }
                        }
                    }
                    None => {
                        conn = None;
                        current_db_key = None;
                        poppler_cache.clear();
                        pdf_surface_cache.borrow_mut().clear();
                        break;
                    }
                }
            }
        }
    });

    let s_cache = state.clone();
    let lb_refresh = page_listbox.clone();
    rx_res.attach(None, move |(idx, pixels)| {
        let st = s_cache.borrow();
        st.pending_thumbnails.borrow_mut().remove(&idx);
        
        let mut surf = cairo::ImageSurface::create(cairo::Format::ARgb32, 100, 140).unwrap();
        {
            let mut data = surf.data().unwrap();
            data.copy_from_slice(&pixels);
        }
        st.thumbnail_cache.borrow_mut().insert(idx, surf);
        
        lb_refresh.queue_draw(); // Aggiorna graficamente
        glib::ControlFlow::Continue
    });
    // --- FINE THREAD BACKGROUND ---

   page_listbox
}

pub fn refresh_sidebar(
    state: &Rc<RefCell<AppState>>, 
    listbox: &gtk::ListBox, 
    canvas: &gtk::DrawingArea,
    spin_page: &gtk::SpinButton,
    lbl_tot: &gtk::Label
) {
    for child in listbox.children() {
        listbox.remove(&child);
    }

    let page_count = state.borrow().page_count;

    for i in 0..page_count {
        let row = gtk::ListBoxRow::new();
        let event_box = gtk::EventBox::new();
        
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
        vbox.set_margin_top(12);
        vbox.set_margin_bottom(12);
        vbox.set_halign(gtk::Align::Center);
        
        let thumb_canvas = gtk::DrawingArea::new();
        thumb_canvas.set_size_request(100, 140); 
        
        // --- LOGICA NOMI E SEGNALIBRI ---
        let mut page_label = format!("Pagina {}", i + 1);
        let mut is_bk = false;
        let mut current_page_id = -1;

        if let Some(conn) = &state.borrow().db {
            if let Ok(page_id) = page_id_at(conn, i) {
                current_page_id = page_id;
                // Query leggerissima per leggere solo il nome senza toccare i dati di disegno
                if let Ok((b_val, b_name)) = conn.query_row(
                    "SELECT is_bookmarked, bookmark_name FROM pages WHERE id = ?1",
                    rusqlite::params![page_id],
                    |r| Ok((r.get::<_, i64>(0).unwrap_or(0), r.get::<_, Option<String>>(1).unwrap_or(None)))
                ) {
                    is_bk = b_val != 0;
                    if is_bk {
                        if let Some(name) = b_name {
                            page_label = name;
                        }
                    }
                }
            }
        }

        let s_clone = state.clone();
        let p_index = i; // Usiamo l'indice della pagina

        thumb_canvas.connect_draw(move |_, cr| {
            let st = s_clone.borrow();
            
            if let Some(surf) = st.thumbnail_cache.borrow().get(&p_index) {
                cr.set_source_surface(surf, 0.0, 0.0).unwrap();
                cr.paint().unwrap();
            } else {
                cr.set_source_rgb(0.9, 0.9, 0.92); // Quadrato di attesa grigio
                cr.paint().unwrap();
                
                let mut pending = st.pending_thumbnails.borrow_mut();
                if !pending.contains(&p_index) {
                    pending.insert(p_index);
                    if let (Some(tx), Some(db_path)) = (&st.thumb_wakeup_tx, &st.db_tmp_path) {
                        // 1. Inseriamo la pagina in CIMA alla lista, insieme alla generazione
                        // corrente del documento (serve al worker per capire se deve
                        // riaprire la connessione anche se il path è lo stesso).
                        st.thumb_req_stack.lock().unwrap().push((db_path.clone(), p_index, st.doc_generation));
                        // 2. Svegliamo il worker
                        let _ = tx.send(());
                    }
                }
            }
            Propagation::Proceed
        });


        // Contenitore Orizzontale per Nome + Tasto Cancella Segnalibro
        let label = gtk::Label::new(Some(&page_label));
        label.set_line_wrap(true);
        label.set_max_width_chars(15);
        label.set_justify(gtk::Justification::Center);

        let label_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        label_box.set_halign(gtk::Align::Center);
        label_box.pack_start(&label, true, true, 0);

        // Se è un segnalibro, creiamo il pulsante per rimuoverlo
        if is_bk {
            let btn_rm = gtk::Button::from_icon_name(Some("edit-delete-symbolic"), gtk::IconSize::Button);
            btn_rm.set_tooltip_text(Some("Rimuovi dai Segnalibri"));
            btn_rm.set_relief(gtk::ReliefStyle::None);
            
            let s_rm = state.clone(); let lb_rm = listbox.clone(); let c_rm = canvas.clone();
            let sp_rm = spin_page.clone(); let lt_rm = lbl_tot.clone();
            let p_id = current_page_id;
            
            btn_rm.connect_clicked(move |_| {
                let mut st = s_rm.borrow_mut();
                if let Some(conn) = &st.db {
                    // Impostiamo is_bookmarked a 0 e nome a NULL nel DB
                    let _ = update_bookmark_status(conn, p_id, false, None);
                }
                
                // Se stavamo guardando proprio questa pagina, aggiorniamo il tasto della toolbar
                if st.current_page_id == p_id {
                    st.current_page_data.is_bookmarked = false;
                    st.current_page_data.bookmark_name = None;
                    if let Some(cb) = &st.update_bookmark_ui { cb(false); }
                }
                
                st.rebuild_bookmark_index();
                drop(st);
                
                // Ridisegniamo la sidebar: il nome tornerà automaticamente a "Pagina N" in base all'ordine attuale!
                refresh_sidebar(&s_rm, &lb_rm, &c_rm, &sp_rm, &lt_rm);
                lb_rm.invalidate_filter();
            });
            label_box.pack_start(&btn_rm, false, false, 0);
        }

        vbox.pack_start(&thumb_canvas, false, false, 0);
        vbox.pack_start(&label_box, false, false, 0);
        event_box.add(&vbox);
        row.add(&event_box);
        
        row.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &*DND_TARGETS, gdk::DragAction::MOVE);
        row.drag_dest_set(gtk::DestDefaults::ALL, &*DND_TARGETS, gdk::DragAction::MOVE);

        row.connect_drag_data_get(move |_, _, data, _, _| {
            data.set_text(&i.to_string());
        });

        let s_drop = state.clone(); let lb_drop = listbox.clone(); let c_drop = canvas.clone();
        let sp_drop = spin_page.clone(); let lt_drop = lbl_tot.clone();
        row.connect_drag_data_received(move |_, _, _, _, data, _, _| {
            if let Some(text) = data.text() {
                if let Ok(source_index) = text.parse::<usize>() {
                    let mut st = s_drop.borrow_mut();
                    if let Some(conn) = &st.db {
                        let source_id = page_id_at(conn, source_index).unwrap();
                        let _ = move_page(conn, source_id, i);
                        
                        if st.current_page == source_index || st.current_page == i {
                            let cur_id = page_id_at(conn, i).unwrap();
                            let page_data = load_page(conn, cur_id).unwrap();
                            st.current_page = i; st.current_page_id = cur_id; st.current_page_data = page_data;
                        }
                    }
                    drop(st);
                    refresh_sidebar(&s_drop, &lb_drop, &c_drop, &sp_drop, &lt_drop);
                    c_drop.queue_draw();
                }
            }
        });

        let s_click = state.clone(); let c_click = canvas.clone(); let sp_click = spin_page.clone();
        let s_menu = state.clone(); let lb_menu = listbox.clone(); let c_menu = canvas.clone();
        let sp_menu = spin_page.clone(); let lt_menu = lbl_tot.clone();

        event_box.connect_button_press_event(move |_, event| {
            if event.button() == 1 { 
                let mut st = s_click.borrow_mut();
                let _ = st.switch_to_page(i);
                drop(st);
                sp_click.set_value((i + 1) as f64);
                c_click.queue_draw();
                return Propagation::Proceed; 

            } else if event.button() == 3 { 
                let menu = gtk::Menu::new();
                
                let item_up = gtk::MenuItem::with_label("Inserisci pagina sopra");
                let s1 = s_menu.clone(); let lb1 = lb_menu.clone(); let c1 = c_menu.clone();
                let sp1 = sp_menu.clone(); let lt1 = lt_menu.clone();
                item_up.connect_activate(move |_| {
                    let mut st = s1.borrow_mut();
                    if let Some(conn) = &st.db { 
                        let bg = st.paper_background.clone();
                        let _ = insert_page_before(conn, i, &bg); 
                        st.page_count += 1; 
                    }
                    if st.current_page >= i { st.current_page += 1; }
                    let pc = st.page_count; let cur = st.current_page; drop(st);
                    sp1.set_range(1.0, pc as f64); sp1.set_value((cur + 1) as f64); lt1.set_text(&format!("di {}", pc));
                    refresh_sidebar(&s1, &lb1, &c1, &sp1, &lt1);
                });

                let item_down = gtk::MenuItem::with_label("Inserisci pagina sotto");
                let s2 = s_menu.clone(); let lb2 = lb_menu.clone(); let c2 = c_menu.clone();
                let sp2 = sp_menu.clone(); let lt2 = lt_menu.clone();
                item_down.connect_activate(move |_| {
                    let mut st = s2.borrow_mut();
                    if let Some(conn) = &st.db { 
                        let bg = st.paper_background.clone();
                        let _ = insert_page_after(conn, i, &bg); 
                        st.page_count += 1; 
                    }
                    if st.current_page > i { st.current_page += 1; }
                    let pc = st.page_count; let cur = st.current_page; drop(st);
                    sp2.set_range(1.0, pc as f64); sp2.set_value((cur + 1) as f64); lt2.set_text(&format!("di {}", pc));
                    refresh_sidebar(&s2, &lb2, &c2, &sp2, &lt2);
                });

                let item_move_up = gtk::MenuItem::with_label("Sposta su");
                let s4 = s_menu.clone(); let lb4 = lb_menu.clone(); let c4 = c_menu.clone();
                let sp4 = sp_menu.clone(); let lt4 = lt_menu.clone();
                item_move_up.set_sensitive(i > 0);
                item_move_up.connect_activate(move |_| {
                    let mut st = s4.borrow_mut();
                    if i == 0 { return; }
                    if let Some(conn) = &st.db {
                        if let Ok(page_id) = page_id_at(conn, i) {
                            let _ = move_page(conn, page_id, i - 1);
                        }
                    }
                    let was_current = st.current_page == i;
                    if was_current {
                        st.current_page = usize::MAX;
                        let _ = st.switch_to_page(i - 1);
                    } else if st.current_page == i - 1 {
                        st.current_page = usize::MAX;
                        let _ = st.switch_to_page(i);
                    }
                    let pc = st.page_count; let cur = st.current_page; drop(st);
                    sp4.set_range(1.0, pc as f64); sp4.set_value((cur + 1) as f64); lt4.set_text(&format!("di {}", pc));
                    refresh_sidebar(&s4, &lb4, &c4, &sp4, &lt4);
                    c4.queue_draw();
                });

                let item_move_down = gtk::MenuItem::with_label("Sposta giù");
                let s5 = s_menu.clone(); let lb5 = lb_menu.clone(); let c5 = c_menu.clone();
                let sp5 = sp_menu.clone(); let lt5 = lt_menu.clone();
                let page_count_now = s_menu.borrow().page_count;
                item_move_down.set_sensitive(i + 1 < page_count_now);
                item_move_down.connect_activate(move |_| {
                    let mut st = s5.borrow_mut();
                    if i + 1 >= st.page_count { return; }
                    if let Some(conn) = &st.db {
                        if let Ok(page_id) = page_id_at(conn, i) {
                            let _ = move_page(conn, page_id, i + 1);
                        }
                    }
                    let was_current = st.current_page == i;
                    if was_current {
                        st.current_page = usize::MAX;
                        let _ = st.switch_to_page(i + 1);
                    } else if st.current_page == i + 1 {
                        st.current_page = usize::MAX;
                        let _ = st.switch_to_page(i);
                    }
                    let pc = st.page_count; let cur = st.current_page; drop(st);
                    sp5.set_range(1.0, pc as f64); sp5.set_value((cur + 1) as f64); lt5.set_text(&format!("di {}", pc));
                    refresh_sidebar(&s5, &lb5, &c5, &sp5, &lt5);
                    c5.queue_draw();
                });

                let item_delete = gtk::MenuItem::with_label("Elimina pagina");
                let s3 = s_menu.clone(); let lb3 = lb_menu.clone(); let c3 = c_menu.clone();
                let sp3 = sp_menu.clone(); let lt3 = lt_menu.clone();
                item_delete.connect_activate(move |_| {
                    let (count, cur) = { let st = s3.borrow(); (st.page_count, st.current_page) };
                    if count > 1 {
                        let mut st = s3.borrow_mut();
                        if let Some(conn) = &st.db {
                            if let Ok(id_to_del) = page_id_at(conn, i) { let _ = delete_page(conn, id_to_del); }
                        }
                        st.page_count -= 1;
                        let new_idx = if cur == i {
                            if i >= st.page_count { st.page_count - 1 } else { i }
                        } else if cur > i { cur - 1 } else { cur };

                        st.current_page = usize::MAX; 
                        let _ = st.switch_to_page(new_idx);
                        
                        let pc = st.page_count; let c_page = st.current_page; drop(st);
                        sp3.set_range(1.0, pc as f64); sp3.set_value((c_page + 1) as f64); lt3.set_text(&format!("di {}", pc));
                        refresh_sidebar(&s3, &lb3, &c3, &sp3, &lt3);
                        c3.queue_draw();
                    }
                });

                
                menu.append(&item_up);
                menu.append(&item_down);
                menu.append(&gtk::SeparatorMenuItem::new());
                menu.append(&item_move_up);
                menu.append(&item_move_down);
                menu.append(&gtk::SeparatorMenuItem::new());
                menu.append(&item_delete);
                menu.show_all();
                menu.popup_easy(event.button(), event.time());
                return Propagation::Stop;
            }
            Propagation::Proceed
        });

        listbox.add(&row);
    }
    listbox.show_all();
}