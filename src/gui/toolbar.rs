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

use crate::save_handler::database_pdf_utilities::*;
use crate::save_handler::database_utilities::*;
use crate::save_handler::db::*;

pub(crate) fn setup_toolbar(
    builder: &gtk::Builder,
    state: &Rc<RefCell<AppState>>,
    window: &gtk::Window,
    canvas: &gtk::DrawingArea,
    spin_page: &gtk::SpinButton,
    lbl_tot: &gtk::Label,
    page_listbox: &gtk::ListBox,
) -> (gtk::Button, gtk::Button) {
    let tool_bar: gtk::Toolbar = builder.object("toolbar").expect("Toolbar not found");

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

    let btn_save = make_btn("document-save.svg", "Save document (.rastin)");
    let btn_open = make_btn("document-open.svg", "Open document");
    add_item!(&btn_save);
    add_item!(&btn_open);
    add_sep!();

    let btn_tool_pen = make_toggle("tool-pencil.svg", "Pen");
    let btn_tool_eraser = make_toggle("tool-eraser.svg", "Eraser");
    let btn_tool_text = make_toggle("tool-text.svg", "Text");
    let btn_tool_select = make_toggle("select-rect.svg", "Selection");
    let btn_tool_shape = make_toggle("tool-shape.svg", "Shapes");
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
        (ShapeKind::Rectangle, "Draw Rectangle"),
        (ShapeKind::Ellipse, "Draw Ellipse"),
        (ShapeKind::Arrow, "Draw Arrow"),
        (ShapeKind::DoubleArrow, "Draw Double Arrow"),
        (ShapeKind::Line, "Draw Line"),
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
        (Color::new(0.0, 0.0, 0.0), "Black"),
        (Color::new(0.85, 0.15, 0.15), "Red"),
        (Color::new(0.15, 0.35, 0.85), "Blue"),
        (Color::new(0.1, 0.65, 0.2), "Green"),
        (Color::new(0.95, 0.6, 0.05), "Orange"),
        (Color::new(0.55, 0.15, 0.75), "Purple"),
    ];

    let mut color_toggles = Vec::new();
    for (color, label) in &preset_colors {
        let btn = make_color_button(color, label);
        btn.set_relief(gtk::ReliefStyle::None);
        color_toggles.push((color.clone(), btn));
    }

    for (c, btn) in &color_toggles {
        let s = state.clone();
        let c_clone = c.clone();
        let all_toggles = color_toggles.iter().map(|(_, b)| b.clone()).collect::<Vec<_>>();
        let c_canvas = canvas.clone();
        
        btn.connect_toggled(move |b| {
            if b.is_active() {
                let mut changed = false;
                let mut switch_to_pen = false;

                
                
                {
                    let mut st = s.borrow_mut();
                    if st.current_color == c_clone { return; } 
                    st.current_color = c_clone.clone();

                    let indices = st.selected_indices.clone();
                    let mut texts_to_invalidate = Vec::new(); 

                    for idx in indices {
                        if let Some(comp) = st.current_page_data.components.get_mut(idx) {
                            match comp {
                                ComponentPayload::PenStroke(stroke) => { stroke.color = c_clone.clone(); changed = true; }
                                ComponentPayload::Shape(shape) => { shape.color = c_clone.clone(); changed = true; }
                                ComponentPayload::RichText(block) => {
                                    for span in &mut block.spans { span.style.color = c_clone.clone(); } 
                                    
                                    
                                    texts_to_invalidate.push(format!("txt_{}", block.id_temporaneo));
                                    changed = true;
                                }
                                _ => {}
                            }
                        }
                    }

                    
                    
                    for id in texts_to_invalidate {
                        st.image_cache.borrow_mut().remove(&id);
                    }

                    if changed {
                        if let Some(conn) = &st.db {
                            let blob = encode_payload_list(&st.current_page_data.components);
                            let _ = conn.execute("DELETE FROM component_rtree WHERE id IN (SELECT id FROM active_components WHERE page_id = ?1)", rusqlite::params![st.current_page_id]);
                            let _ = conn.execute("DELETE FROM active_components WHERE page_id = ?1", rusqlite::params![st.current_page_id]);
                            let _ = conn.execute("UPDATE base_layers SET baked_blob = ?1 WHERE page_id = ?2", rusqlite::params![blob, st.current_page_id]);
                        }
                        st.is_modified = true;
                        st.undo_stack.clear();
                        st.redo_stack.clear();
                    }

                    
                    if st.active_tool == Tool::Eraser {
                        st.active_tool = Tool::Pen;
                        switch_to_pen = true;
                    }
                } 

                
                
                for other in &all_toggles {
                    if other != b {
                        other.set_active(false);
                    }
                }

                if changed {
                    c_canvas.queue_draw();
                }

                
                
                if switch_to_pen {
                    let cb = s.borrow().update_toolbar_ui.clone();
                    if let Some(f) = cb {
                        f(&Tool::Pen);
                    }
                }
            } else {
                
                if s.borrow().current_color == c_clone {
                    b.set_active(true);
                }
            }
        });
        add_item!(btn);
    }
    
    
    if let Some((_, first_btn)) = color_toggles.first() {
        first_btn.set_active(true);
    }

    let btn_custom_color = gtk::Button::with_label("🎨");
    {
        let s = state.clone();
        let w = window.clone();
        let c_canvas = canvas.clone();
        let all_toggles = color_toggles.iter().map(|(_, b)| b.clone()).collect::<Vec<_>>();
        
        btn_custom_color.connect_clicked(move |_| {
            let dialog = gtk::ColorChooserDialog::new(Some("Choose a color"), Some(&w));
            if dialog.run() == gtk::ResponseType::Ok {
                let rgba = dialog.rgba();
                let new_c = Color::new(rgba.red(), rgba.green(), rgba.blue());
                
                let mut changed = false;
                let mut switch_to_pen = false;

                
                {
                    let mut st = s.borrow_mut();
                    st.current_color = new_c.clone();

                    let indices = st.selected_indices.clone();
                    for idx in indices {
                        if let Some(comp) = st.current_page_data.components.get_mut(idx) {
                            match comp {
                                ComponentPayload::PenStroke(stroke) => { stroke.color = new_c.clone(); changed = true; }
                                ComponentPayload::Shape(shape) => { shape.color = new_c.clone(); changed = true; }
                                ComponentPayload::RichText(block) => { 
                                    for span in &mut block.spans { span.style.color = new_c.clone(); }
                                    changed = true; 
                                }
                                _ => {}
                            }
                        }
                    }

                    if changed {
                        if let Some(conn) = &st.db {
                            let blob = encode_payload_list(&st.current_page_data.components);
                            let _ = conn.execute("DELETE FROM component_rtree WHERE id IN (SELECT id FROM active_components WHERE page_id = ?1)", rusqlite::params![st.current_page_id]);
                            let _ = conn.execute("DELETE FROM active_components WHERE page_id = ?1", rusqlite::params![st.current_page_id]);
                            let _ = conn.execute("UPDATE base_layers SET baked_blob = ?1 WHERE page_id = ?2", rusqlite::params![blob, st.current_page_id]);
                        }
                        st.is_modified = true;
                        st.undo_stack.clear();
                        st.redo_stack.clear();
                    }

                    if st.active_tool == Tool::Eraser {
                        st.active_tool = Tool::Pen;
                        switch_to_pen = true;
                    }
                } 

                
                for other in &all_toggles {
                    other.set_active(false);
                }

                if changed {
                    c_canvas.queue_draw();
                }

                if switch_to_pen {
                    let cb = s.borrow().update_toolbar_ui.clone();
                    if let Some(f) = cb {
                        f(&Tool::Pen);
                    }
                }
            }
            unsafe { dialog.destroy(); }
        });
    }
    add_item!(&btn_custom_color);
    add_sep!();
    
    let btn_thin = make_toggle("thickness-fine.svg", "Thin");
    let btn_med = make_toggle("thickness-medium.svg", "Medium");
    let btn_thick = make_toggle("thickness-thick.svg", "Thick");
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

    let btn_add_page = make_btn("page-add.svg", "Add new page");
    
    let btn_bookmark = gtk::ToggleButton::new();
    btn_bookmark.set_image(Some(&gtk::Image::from_icon_name(Some("bookmark-new"), gtk::IconSize::Button)));
    btn_bookmark.set_tooltip_text(Some("Add/Remove bookmarks"));
    btn_bookmark.set_always_show_image(true);
    btn_bookmark.set_relief(gtk::ReliefStyle::None);

    let btn_del_page = make_btn("page-delete.svg", "Delete current page");

    {
        let s = state.clone();
        let w = window.clone();
        let lb = page_listbox.clone();
        let btn = btn_bookmark.clone();

        btn.connect_toggled(move |b| {
            let is_active = b.is_active();

            
            let (needs_update, page_id) = {
                let st = s.borrow();
                if st.current_page_data.is_bookmarked == is_active {
                    (false, 0)
                } else {
                    (true, st.current_page_id)
                }
            };

            
            if !needs_update { return; }

            let mut custom_name = None;
            let mut user_cancelled = false;

            
            if is_active {
                let dialog = gtk::Dialog::with_buttons(
                    Some("New bookmark"),
                    Some(&w),
                    gtk::DialogFlags::MODAL,
                    &[("Cancel", gtk::ResponseType::Cancel), ("Save", gtk::ResponseType::Ok)],
                );
                let content_area = dialog.content_area();
                let entry = gtk::Entry::new();
                entry.set_placeholder_text(Some("Add a name (optional)"));
                entry.set_margin_top(10);
                entry.set_margin_bottom(10);
                entry.set_margin_start(10);
                entry.set_margin_end(10);
                content_area.pack_start(&entry, true, true, 0);
                dialog.show_all();

                
                
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

            
            let mut st = s.borrow_mut();

            if user_cancelled {
                st.current_page_data.is_bookmarked = false;
                drop(st); 
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
                let _ = update_bookmark_status(conn, page_id, is_active, custom_name.as_deref());
            }

            st.rebuild_bookmark_index();
            
            
            drop(st); 
            lb.invalidate_filter(); 
        });

    }

    
    
    let cb_bookmark = btn_bookmark.clone();
    state.borrow_mut().update_bookmark_ui = Some(Rc::new(move |is_bk| {
        let cb = cb_bookmark.clone();
        
        
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
                        
                        
                    }
                    Err(e) => eprintln!("Error adding page: {e}"),
                }
            }

            st.thumbnail_cache.borrow_mut().clear();
            st.pending_thumbnails.borrow_mut().clear();
            st.doc_generation += 1;

            let page_count = st.page_count;
            let current_page = st.current_page;
            drop(st);
            sp.set_range(1.0, page_count as f64);
            sp.set_value((current_page + 1) as f64);
            lt.set_text(&format!("of {}", page_count));
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
                    
                    
                }
                let page_count = st.page_count;
                let current_page = st.current_page;

                st.thumbnail_cache.borrow_mut().clear();
                st.pending_thumbnails.borrow_mut().clear();
                st.doc_generation += 1;

                drop(st);
                sp.set_range(1.0, page_count as f64);
                sp.set_value((current_page + 1) as f64);
                lt.set_text(&format!("of {}", page_count));
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
