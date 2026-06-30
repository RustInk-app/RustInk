pub mod canvas_events;
pub mod drawing;
pub mod file_ops;
pub mod shortcuts;
pub mod sidebar;
pub mod state;
pub mod toolbar;
pub mod utils;

use glib::clone;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::save_handler::autosave::*;
use crate::save_handler::db::*;

use crate::check_recovery;

use crate::gui::canvas_events::*;
use crate::gui::drawing::*;
use crate::gui::file_ops::*;
use crate::gui::shortcuts::*;
use crate::gui::sidebar::*;
use crate::gui::state::*;
use crate::gui::toolbar::*;

use crate::models::page::PaperBackground;

use glib::Propagation;

pub fn build_ui(app: &gtk::Application) {
    
    let state = Rc::new(RefCell::new(AppState::new()));

    match check_recovery() 
    {
        Some((backup_path, original_bundle)) => {
            let tmp = temp_db_dir();
            let recovered = import_bundle(&backup_path, &tmp)
                .map_err(|e| e.to_string())
                .and_then(|_| rusqlite::Connection::open(&tmp).map_err(|e| e.to_string()));

            match recovered {
                Ok(conn) => {
                    let count = page_count(&conn).unwrap_or(1);
                    let first_id = page_id_at(&conn, 0).unwrap_or(1);
                    let first_page = load_page(&conn, first_id).unwrap_or_default();

                    let mut st = state.borrow_mut();
                    st.page_count = count;
                    st.current_page = 0;
                    st.current_page_id = first_id;
                    st.paper_background = first_page.background.clone();
                    st.current_page_data = first_page;
                    st.bundle_path = original_bundle.clone();
                    st.is_modified = true; // recuperato ma non ancora ri-salvato nel file originale
                    st.db_tmp_path = Some(tmp);
                    st.db = Some(conn);
                    st.undo_stack.clear();
                    st.redo_stack.clear();

                    if let Some(bp) = &original_bundle {
                        if bp.extension().and_then(|e| e.to_str()) == Some("rastin") {
                            let _ = st.acquire_lock(bp);
                        }
                    }
                    drop(st);

                    // Il backup è stato importato: la vecchia cartella di sessione crashata può sparire
                    clear_old_sessions();
                }
                Err(e) => {
                    eprintln!("[RECOVERY] Errore import backup: {e}");
                    if let Err(e) = state.borrow_mut().init_new_document() {
                        eprintln!("Errore inizializzazione DB: {e}");
                    }
                }
            }
        }
        None => {
            if let Err(e) = state.borrow_mut().init_new_document() {
                eprintln!("Errore inizializzazione DB: {e}");
            }
        }
    }

    let glade_src = include_str!("ui/menu.glade");
    let builder = gtk::Builder::from_string(glade_src);

    let window: gtk::Window = builder
        .object("rastin_window")
        .expect("rastin_window non trovata");
    app.add_window(&window);
    window.set_title(&state.borrow().window_title());
    window.set_default_size(1280, 800);

    utils::load_css();

    let canvas: gtk::DrawingArea = builder
        .object("first_panel_drawing_area")
        .expect("canvas non trovato");
    canvas.add_events(
        gtk::gdk::EventMask::BUTTON_PRESS_MASK
        | gtk::gdk::EventMask::BUTTON_RELEASE_MASK
        | gtk::gdk::EventMask::POINTER_MOTION_MASK
        | gtk::gdk::EventMask::SCROLL_MASK,
    );

    // Navigate from a page to another
    let spin_page: gtk::SpinButton = builder.object("spin_page").expect("spin_page non trovato");

    let lbl_tot: gtk::Label = builder.object("lbl_tot").expect("lbl_tot non trovato");

    // zoom_adj tracks Zoom slider
    let zoom_adj: gtk::Adjustment = builder.object("zoom_adj").expect("zoom_adj non trovato");
    
    // zoom_pct outputs zoom_adj value
    let zoom_pct: gtk::Label = builder
        .object("lbl_zoom_pct")
        .expect("lbl_zoom_pct non trovato");

    {
        let s = state.clone();
        let c = canvas.clone();
        let zp = zoom_pct.clone();
        zoom_adj.connect_value_changed(move |adj| {
            let zoom = adj.value();
            {
                let mut st = s.borrow_mut();
                st.zoom = zoom;
                st.scroll_offset_y = 0.0;
            }
            zp.set_text(&format!("{:.0}%", zoom * 100.0));
            c.queue_draw();
        });
    }

    let page_listbox = setup_sidebar(&builder);
    let (btn_save, btn_open) = setup_toolbar(
        &builder,
        &state,
        &window,
        &canvas,
        &spin_page,
        &lbl_tot,
        &page_listbox,
    );

    setup_menus(&builder, &window, &state, &canvas);
    setup_preferences_dialog(&builder, &state);

    setup_canvas_drawing(&canvas, &state);
    setup_canvas_events(&canvas, &state, &window, &spin_page, &lbl_tot);
    setup_keyboard_shortcuts(&window, &state, &canvas);

    setup_file_ops(
        &builder,
        &window,
        &canvas,
        &state,
        &spin_page,
        &lbl_tot,
        &page_listbox,
        &btn_save,
        &btn_open,
    );

    setup_autosave(&state);
    setup_window_close(&window, &state);

    refresh_sidebar(&state, &page_listbox, &canvas, &spin_page, &lbl_tot);

    window.show_all();
}

fn setup_preferences_dialog(builder: &gtk::Builder, state: &Rc<RefCell<AppState>>) {
    let menu_pref: gtk::MenuItem = builder.object("file_preferences").unwrap();
    let dialog: gtk::Dialog = builder.object("preferences_dialog").unwrap();

    let combo1: gtk::ComboBoxText = builder.object("choice_tool_first_button").unwrap();
    let combo2: gtk::ComboBoxText = builder.object("choice_tool_second_button").unwrap();
    let box1: gtk::ButtonBox = builder.object("first_button_event_listener").unwrap();
    let box2: gtk::ButtonBox = builder.object("second_button_event_listener").unwrap();
    let btn_ok: gtk::Button = builder.object("btn_pref_ok").expect("OK non trovato");

    for tool_name in &["Nessuno", "Penna", "Gomma", "Testo", "Seleziona"] {
        combo1.append_text(tool_name);
        combo2.append_text(tool_name);
    }
    combo1.set_active(Some(0));
    combo2.set_active(Some(0));

    let btn_key1 = gtk::Button::with_label("Clicca e premi un tasto...");
    let btn_key2 = gtk::Button::with_label("Clicca e premi un tasto...");
    btn_key1.set_size_request(180, -1);
    btn_key2.set_size_request(180, -1);
    box1.add(&btn_key1);
    box2.add(&btn_key2);

    let setup_listener = |btn: &gtk::Button| {
        btn.connect_clicked(|b| b.set_label("In ascolto..."));

        btn.connect_key_press_event(|b, ev| {
            let name = ev.keyval().name().unwrap_or_else(|| "Sconosciuto".into());
            b.set_label(&name);
            Propagation::Stop
        });

        btn.connect_button_press_event(|b, ev| {
            let btn_num = ev.button();
            if btn_num == 1 {
                return Propagation::Proceed;
            }
            b.set_label(&format!("Mouse Button {}", btn_num));
            Propagation::Stop
        });
    };

    setup_listener(&btn_key1);
    setup_listener(&btn_key2);

    btn_ok.connect_clicked(
        clone!(@weak dialog, @strong state, @weak combo1, @weak combo2 => move |_| {
            let mut st = state.borrow_mut();

            let map_tool = |txt: Option<String>| match txt.as_deref() {
                Some("Penna") => Some(crate::models::page::Tool::Pen),
                Some("Gomma") => Some(crate::models::page::Tool::Eraser),
                Some("Testo") => Some(crate::models::page::Tool::Text),
                Some("Seleziona") => Some(crate::models::page::Tool::Select),
                _ => None,
            };

            st.pref_button_2_tool = map_tool(combo1.active_text().map(|s| s.to_string()));
            st.pref_button_3_tool = map_tool(combo2.active_text().map(|s| s.to_string()));

            dialog.hide();
        }),
    );

    menu_pref.connect_activate(clone!(@weak dialog => move |_| dialog.show_all()));
}

fn setup_menus(
    builder: &gtk::Builder,
    window: &gtk::Window,
    state: &Rc<RefCell<AppState>>,
    canvas: &gtk::DrawingArea,
) {
    let view_sidebar: gtk::CheckMenuItem = builder.object("view_sidebar_option").unwrap();
    view_sidebar.set_active(true);

    let sidebar_container: gtk::Box = builder
        .object("sidebar_container")
        .expect("Box sidebar mancante");
    view_sidebar.connect_toggled(glib::clone!(@weak sidebar_container => move |item| {
        sidebar_container.set_visible(item.is_active());
    }));

    let btn_close_sidebar: gtk::Button = builder
        .object("btn_close_sidebar")
        .expect("Tasto X non trovato");
    btn_close_sidebar.connect_clicked(glib::clone!(@weak view_sidebar => move |_| {
        view_sidebar.set_active(false);
    }));

    let bg_ruled: gtk::RadioMenuItem = builder.object("ruled_option").unwrap();
    let bg_plain: gtk::RadioMenuItem = builder.object("plain_option").unwrap();
    let bg_grid: gtk::RadioMenuItem = builder.object("grid_option").unwrap();

    let s = state.clone();
    let c = canvas.clone();

    let connect_bg = move |item: &gtk::RadioMenuItem, bg_type: PaperBackground| {
        let s_clone = s.clone();
        let c_clone = c.clone();
        item.connect_toggled(move |radio| {
            if radio.is_active() {
                let mut st = s_clone.borrow_mut();
                st.paper_background = bg_type.clone();
                st.current_page_data.background = bg_type.clone();
                st.is_modified = true;
                if let Some(conn) = &st.db {
                    let _ = crate::save_handler::db::update_page_background(
                        conn,
                        st.current_page_id,
                        &bg_type,
                    );
                }
                drop(st);
                c_clone.queue_draw();
            }
        });
    };

    connect_bg(&bg_ruled, PaperBackground::Ruled);
    connect_bg(&bg_plain, PaperBackground::Plain);
    connect_bg(&bg_grid, PaperBackground::Grid);

    let file_quit: gtk::MenuItem = builder.object("file_quit").unwrap();
    let w_clone = window.clone();
    file_quit.connect_activate(move |_| {
        w_clone.close();
    });
}
