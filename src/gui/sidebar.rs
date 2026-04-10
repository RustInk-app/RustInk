use crate::models::page::*;
use crate::save_handler::db::*;
use crate::gui::state::*;
use crate::gui::drawing::draw_page;

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

pub(crate) fn setup_sidebar(builder: &gtk::Builder) -> gtk::ListBox {
    
    let sidebar_scrolled: gtk::ScrolledWindow = builder.object("first_panel_sidebar").expect("first_panel_sidebar non trovata");
    
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
    page_listbox.style_context().add_provider(
        &css_provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    
    sidebar_scrolled.add(&page_listbox);
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
        
        let s_clone = state.clone();
        thumb_canvas.connect_draw(move |_, cr| {
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.paint().unwrap();

            if let Some(conn) = &s_clone.borrow().db {
                if let Ok(page_id) = page_id_at(conn, i) {
                    if let Ok(page_data) = load_page(conn, page_id) {
                        let zoom = 100.0 / PAGE_W;
                        cr.scale(zoom, zoom);
                        draw_page(cr, &page_data, 0.0, 0.0, &s_clone.borrow().image_cache, &page_data.background);
                    }
                }
            }
            Propagation::Proceed
        });

        let label = gtk::Label::new(Some(&format!("Pagina {}", i + 1)));

        vbox.pack_start(&thumb_canvas, false, false, 0);
        vbox.pack_start(&label, false, false, 0);
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