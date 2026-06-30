use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::gui::refresh_sidebar;
use crate::gui::state::AppState;
use crate::gui::utils::{load_icon, make_color_button};

use crate::models::color::*;
use crate::models::page::*;
use crate::models::select::*;
use crate::models::stroke::*;

use crate::save_handler::db::{delete_page, insert_page_after, load_page, page_id_at};

pub(crate) fn setup_toolbar(
    builder: &gtk::Builder,
    state: &Rc<RefCell<AppState>>,
    window: &gtk::Window,
    canvas: &gtk::DrawingArea,
    spin_page: &gtk::SpinButton,
    lbl_tot: &gtk::Label,
    page_listbox: &gtk::ListBox,
) -> (gtk::Button, gtk::Button) {
    let tool_bar: gtk::Toolbar = builder.object("toolbar").expect("Toolbar non trovata");

    macro_rules! add_item {
        ($widget:expr) => {
            let item = gtk::ToolItem::new();
            item.add($widget);
            tool_bar.insert(&item, -1);
        };
    }

    macro_rules! add_sep {
        () => {
            let sep = gtk::SeparatorToolItem::new();
            tool_bar.insert(&sep, -1);
        };
    }

    let make_btn = |icon_name: &str, tooltip: &str| -> gtk::Button {
        let btn = gtk::Button::new();
        btn.set_image(Some(&load_icon(icon_name)));
        btn.set_tooltip_text(Some(tooltip));
        btn.set_always_show_image(true);
        btn.set_relief(gtk::ReliefStyle::None);
        btn
    };
    let make_toggle = |icon_name: &str, tooltip: &str| -> gtk::ToggleButton {
        let btn = gtk::ToggleButton::new();
        btn.set_image(Some(&load_icon(icon_name)));
        btn.set_tooltip_text(Some(tooltip));
        btn.set_always_show_image(true);
        btn.set_relief(gtk::ReliefStyle::None);
        btn
    };

    let btn_save = make_btn("document-save.svg", "Salva il documento (.rastin)");
    let btn_open = make_btn("document-open.svg", "Apri documento");
    add_item!(&btn_save);
    add_item!(&btn_open);
    add_sep!();

    let btn_tool_pen = make_toggle("tool-pencil.svg", "Penna");
    let btn_tool_eraser = make_toggle("tool-eraser.svg", "Gomma");
    let btn_tool_text = make_toggle("tool-text.svg", "Testo");
    let btn_tool_select = make_toggle("select-rect.svg", "Seleziona");
    let btn_tool_shape = make_toggle("tool-shape.svg", "Forme");
    btn_tool_pen.set_active(true);

    {
        let s = state.clone();
        let be = btn_tool_eraser.clone();
        let bt = btn_tool_text.clone();
        let bs = btn_tool_select.clone();
        btn_tool_pen.connect_toggled(move |b| {
            if b.is_active() {
                be.set_active(false);
                bt.set_active(false);
                bs.set_active(false);
                s.borrow_mut().active_tool = Tool::Pen;
            }
        });
    }
    {
        let s = state.clone();
        let bp = btn_tool_pen.clone();
        let bt = btn_tool_text.clone();
        let bs = btn_tool_select.clone();
        btn_tool_eraser.connect_toggled(move |b| {
            if b.is_active() {
                bp.set_active(false);
                bt.set_active(false);
                bs.set_active(false);
                s.borrow_mut().active_tool = Tool::Eraser;
            }
        });
    }
    {
        let s = state.clone();
        let bp = btn_tool_pen.clone();
        let be = btn_tool_eraser.clone();
        let bs = btn_tool_select.clone();
        btn_tool_text.connect_toggled(move |b| {
            if b.is_active() {
                bp.set_active(false);
                be.set_active(false);
                bs.set_active(false);
                s.borrow_mut().active_tool = Tool::Text;
            }
        });
    }
    {
        let s = state.clone();
        let bp = btn_tool_pen.clone();
        let be = btn_tool_eraser.clone();
        let bt = btn_tool_text.clone();
        btn_tool_select.connect_toggled(move |b| {
            if b.is_active() {
                bp.set_active(false);
                be.set_active(false);
                bt.set_active(false);
                let mut st = s.borrow_mut();
                st.active_tool = Tool::Select;
                st.selected_index = None;
                st.drag_mode = DragMode::None;
            }
        });
    }
    let shape_menu = gtk::Menu::new();
    let shape_items: [(ShapeKind, &str); 5] = [
        (ShapeKind::Rectangle, "Disegna Rettangolo"),
        (ShapeKind::Ellipse, "Disegna Ellisse"),
        (ShapeKind::Arrow, "Disegna Freccia"),
        (ShapeKind::DoubleArrow, "Disegna Doppia Freccia"),
        (ShapeKind::Line, "Disegna Linea Retta"),
    ];
    for (kind, label) in shape_items {
        let item = gtk::MenuItem::with_label(label);
        let s = state.clone();
        item.connect_activate(move |_| {
            s.borrow_mut().active_tool = Tool::Shape(kind.clone());
        });
        shape_menu.append(&item);
    }
    shape_menu.show_all();

    {
        let s = state.clone();
        let bp = btn_tool_pen.clone();
        let be = btn_tool_eraser.clone();
        let bt = btn_tool_text.clone();
        let bsel = btn_tool_select.clone();
        let menu = shape_menu.clone();
        btn_tool_shape.connect_toggled(move |b| {
            if b.is_active() {
                bp.set_active(false);
                be.set_active(false);
                bt.set_active(false);
                bsel.set_active(false);
                if !matches!(s.borrow().active_tool, Tool::Shape(_)) {
                    s.borrow_mut().active_tool = Tool::Shape(ShapeKind::Line);
                }
                menu.popup_at_widget(b, gtk::gdk::Gravity::SouthWest, gtk::gdk::Gravity::NorthWest, None);
            }
        });
    }

    add_item!(&btn_tool_pen);
    add_item!(&btn_tool_eraser);
    add_item!(&btn_tool_text);
    add_item!(&btn_tool_select);
    add_item!(&btn_tool_shape);
    add_sep!();

    let preset_colors = vec![
        (Color::new(0.0, 0.0, 0.0), "Nero"),
        (Color::new(0.85, 0.15, 0.15), "Rosso"),
        (Color::new(0.15, 0.35, 0.85), "Blu"),
        (Color::new(0.1, 0.65, 0.2), "Verde"),
        (Color::new(0.95, 0.6, 0.05), "Arancione"),
        (Color::new(0.55, 0.15, 0.75), "Viola"),
    ];
    for (color, label) in &preset_colors {
        let btn = make_color_button(color, label);
        btn.set_relief(gtk::ReliefStyle::None);
        let s = state.clone();
        let c = color.clone();
        btn.connect_clicked(move |_| {
            s.borrow_mut().current_color = c.clone();
        });
        add_item!(&btn);
    }

    let btn_custom_color = gtk::Button::with_label("🎨");
    {
        let s = state.clone();
        let w = window.clone();
        btn_custom_color.connect_clicked(move |_| {
            let dialog = gtk::ColorChooserDialog::new(Some("Scegli un colore"), Some(&w));
            if dialog.run() == gtk::ResponseType::Ok {
                let rgba = dialog.rgba();
                s.borrow_mut().current_color = Color::new(rgba.red(), rgba.green(), rgba.blue());
            }
            unsafe {
                dialog.destroy();
            }
        });
    }
    add_item!(&btn_custom_color);
    add_sep!();

    let btn_thin = make_toggle("thickness-fine.svg", "Sottile");
    let btn_med = make_toggle("thickness-medium.svg", "Medio");
    let btn_thick = make_toggle("thickness-thick.svg", "Grande");
    btn_med.set_active(true);

    {
        let s = state.clone();
        let bm = btn_med.clone();
        let bt = btn_thick.clone();
        btn_thin.connect_toggled(move |b| {
            if b.is_active() {
                bm.set_active(false);
                bt.set_active(false);
                s.borrow_mut().current_width = STROKE_THIN;
            }
        });
    }
    {
        let s = state.clone();
        let bn = btn_thin.clone();
        let bt = btn_thick.clone();
        btn_med.connect_toggled(move |b| {
            if b.is_active() {
                bn.set_active(false);
                bt.set_active(false);
                s.borrow_mut().current_width = STROKE_MEDIUM;
            }
        });
    }
    {
        let s = state.clone();
        let bn = btn_thin.clone();
        let bm = btn_med.clone();
        btn_thick.connect_toggled(move |b| {
            if b.is_active() {
                bn.set_active(false);
                bm.set_active(false);
                s.borrow_mut().current_width = STROKE_THICK;
            }
        });
    }
    add_item!(&btn_thin);
    add_item!(&btn_med);
    add_item!(&btn_thick);

    let btn_add_page = make_btn("page-add.svg", "Aggiungi nuova pagina");
    
    let btn_bookmark = gtk::ToggleButton::new();
    btn_bookmark.set_image(Some(&gtk::Image::from_icon_name(Some("bookmark-new"), gtk::IconSize::Button)));
    btn_bookmark.set_tooltip_text(Some("Imposta/Rimuovi Segnalibro"));
    btn_bookmark.set_always_show_image(true);
    btn_bookmark.set_relief(gtk::ReliefStyle::None);

    let btn_del_page = make_btn("page-delete.svg", "Elimina pagina corrente");

    {
        let s = state.clone();
        let w = window.clone();
        let lb = page_listbox.clone();
        let btn = btn_bookmark.clone();

        btn.connect_toggled(move |b| {
            let is_active = b.is_active();

            // 1. Controlliamo se serve fare qualcosa usando un borrow CORTISSIMO (solo lettura)
            let (needs_update, page_id) = {
                let st = s.borrow();
                if st.current_page_data.is_bookmarked == is_active {
                    (false, 0)
                } else {
                    (true, st.current_page_id)
                }
            };

            // Se non c'è nulla da aggiornare usciamo subito
            if !needs_update { return; }

            let mut custom_name = None;
            let mut user_cancelled = false;

            // 2. Apriamo il popup QUANDO NESSUN BORROW E' ATTIVO!
            if is_active {
                let dialog = gtk::Dialog::with_buttons(
                    Some("Nuovo Segnalibro"),
                    Some(&w),
                    gtk::DialogFlags::MODAL,
                    &[("Annulla", gtk::ResponseType::Cancel), ("Salva", gtk::ResponseType::Ok)],
                );
                let content_area = dialog.content_area();
                let entry = gtk::Entry::new();
                entry.set_placeholder_text(Some("Inserisci un nome (opzionale)"));
                entry.set_margin_top(10);
                entry.set_margin_bottom(10);
                entry.set_margin_start(10);
                entry.set_margin_end(10);
                content_area.pack_start(&entry, true, true, 0);
                dialog.show_all();

                // L'app aspetta qui, ma lo stato non è bloccato, 
                // quindi le miniature nella sidebar possono disegnarsi liberamente!
                if dialog.run() == gtk::ResponseType::Ok {
                    let text = entry.text().to_string();
                    if !text.is_empty() {
                        custom_name = Some(text);
                    }
                } else {
                    user_cancelled = true;
                }
                unsafe { dialog.destroy(); }
            }

            // 3. Ora che il popup è chiuso, riprendiamo il controllo dello stato per salvare i cambiamenti
            let mut st = s.borrow_mut();

            if user_cancelled {
                st.current_page_data.is_bookmarked = false;
                drop(st); // Fondamentale chiuderlo prima di toccare di nuovo l'UI!
                b.set_active(false);
                return;
            }

            st.current_page_data.is_bookmarked = is_active;
            if is_active {
                st.current_page_data.bookmark_name = custom_name.clone();
            } else {
                st.current_page_data.bookmark_name = None;
            }

            if let Some(conn) = &st.db {
                let _ = crate::save_handler::db::update_bookmark_status(conn, page_id, is_active, custom_name.as_deref());
            }

            st.rebuild_bookmark_index();
            
            // 4. Fondamentale rilasciare il mut prima di invalidare la listbox
            drop(st); 
            lb.invalidate_filter(); // Forza la lista ad aggiornarsi
        });

    }

    // Registriamo la callback per aggiornare il tasto segnalibro quando si cambia pagina!
    // Registriamo la callback per aggiornare il tasto segnalibro quando si cambia pagina!
    let cb_bookmark = btn_bookmark.clone();
    state.borrow_mut().update_bookmark_ui = Some(Rc::new(move |is_bk| {
        let cb = cb_bookmark.clone();
        // Usiamo l'idle_add per ritardare l'aggiornamento visivo di qualche millisecondo,
        // permettendo alla funzione chiamante (come switch_to_page) di rilasciare i borrow!
        gtk::glib::idle_add_local_once(move || {
            cb.set_active(is_bk);
        });
    }));

    add_sep!();
    add_item!(&btn_add_page);
    add_item!(&btn_del_page);
    add_item!(&btn_bookmark);

    {
        let s = state.clone();
        let sp = spin_page.clone();
        let lt = lbl_tot.clone();
        let c = canvas.clone();
        let lb = page_listbox.clone();
        btn_add_page.connect_clicked(move |_| {
            let mut st = s.borrow_mut();
            let cur_order = st.current_page;
            if let Some(conn) = &st.db {
                let current_bg = st.paper_background.clone();
                match insert_page_after(conn, cur_order, &current_bg) {
                    Ok(new_id) => {
                        st.page_count += 1;
                        st.current_page = cur_order + 1;
                        st.current_page_id = new_id;
                        let mut new_page = crate::models::page::PageData::new();
                        new_page.background = current_bg;
                        st.current_page_data = new_page;
                        // st.undo_stack.clear();
                        // st.redo_stack.clear();
                    }
                    Err(e) => eprintln!("Errore inserimento pagina: {e}"),
                }
            }
            let page_count = st.page_count;
            let current_page = st.current_page;
            drop(st);
            sp.set_range(1.0, page_count as f64);
            sp.set_value((current_page + 1) as f64);
            lt.set_text(&format!("di {}", page_count));
            c.queue_draw();
            refresh_sidebar(&s, &lb, &c, &sp, &lt);
        });
    }

    {
        let s = state.clone();
        let sp = spin_page.clone();
        let lt = lbl_tot.clone();
        let c = canvas.clone();
        let lb = page_listbox.clone();
        btn_del_page.connect_clicked(move |_| {
            let (count, cur, page_id) = {
                let st = s.borrow();
                (st.page_count, st.current_page, st.current_page_id)
            };
            if count > 1 {
                let mut st = s.borrow_mut();
                if let Some(conn) = &st.db {
                    let _ = delete_page(conn, page_id);
                }
                st.page_count -= 1;
                let new_idx = if cur >= st.page_count {
                    st.page_count - 1
                } else {
                    cur
                };
                let mut next_page_info = None;
                if let Some(conn) = &st.db {
                    if let Ok(new_id) = page_id_at(conn, new_idx) {
                        let page_data = load_page(conn, new_id).unwrap_or_default();
                        next_page_info = Some((new_id, page_data));
                    }
                }
                if let Some((new_id, page_data)) = next_page_info {
                    st.current_page = new_idx;
                    st.current_page_id = new_id;
                    st.current_page_data = page_data;
                    // st.undo_stack.clear();
                    // st.redo_stack.clear();
                }
                let page_count = st.page_count;
                let current_page = st.current_page;
                drop(st);
                sp.set_range(1.0, page_count as f64);
                sp.set_value((current_page + 1) as f64);
                lt.set_text(&format!("di {}", page_count));
                refresh_sidebar(&s, &lb, &c, &sp, &lt);
            }
            c.queue_draw();
        });
    }

    let cb_pen = btn_tool_pen.clone();
    let cb_eraser = btn_tool_eraser.clone();
    let cb_text = btn_tool_text.clone();
    let cb_sel = btn_tool_select.clone();
    let cb_shape = btn_tool_shape.clone();

    // Creiamo una callback per aggiornare i tasti visivamente
    let update_ui = Rc::new(move |tool: &Tool| {
        match tool {
            Tool::Pen => cb_pen.set_active(true),
            Tool::Eraser => cb_eraser.set_active(true),
            Tool::Text => cb_text.set_active(true),
            Tool::Select => cb_sel.set_active(true),
            Tool::Shape(_) => cb_shape.set_active(true),
        }
    });
    state.borrow_mut().update_toolbar_ui = Some(update_ui);

    (btn_save, btn_open)
}
