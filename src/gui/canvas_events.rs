
use crate::models::textbox::*;
use crate::models::page::*;
use crate::models::stroke::*;
use crate::models::select::*;

use crate::save_handler::db::*;

use gtk::prelude::*;
use gtk::gdk;
use glib::Propagation;

use std::cell::RefCell;
use std::rc::Rc;
use glib::clone; 

use crate::gui::state::*;
use crate::gui::drawing::*;


pub(crate) fn setup_canvas_events(
    canvas: &gtk::DrawingArea,
    state: &Rc<RefCell<AppState>>,
    window: &gtk::Window,
    spin_page: &gtk::SpinButton,
    lbl_tot: &gtk::Label
) {
    
    
    let s = state.clone();
    {
    canvas.connect_button_press_event(clone!(@strong state, @strong window as w, @strong canvas as c => move |_, event| {
        let button = event.button();

        let btn_trigger = EventTrigger::Mouse(button);
        let target_tool = {
            let st = state.borrow();
            if Some(&btn_trigger) == st.pref_trigger_1.as_ref() { st.pref_tool_1.clone() }
            else if Some(&btn_trigger) == st.pref_trigger_2.as_ref() { st.pref_tool_2.clone() }
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
                st_mut.active_temp_trigger = Some(btn_trigger);
                drop(st_mut);

                if let Some(f) = cb { f(&tool); }
                c.queue_draw();
            }
            if button != 1 {
                return Propagation::Stop;
            }
        }

        if button != 1 { return Propagation::Proceed; }

        let (mx, my) = event.position();
        
        let (ox, oy) = state.borrow().page_origin;
        let zoom = state.borrow().zoom;
        let tool = state.borrow().active_tool.clone();

        
        let px = (mx - ox) / zoom;
        let py = (my - oy) / zoom;

        
        if px < 0.0 || px > PAGE_W || py < 0.0 || py > PAGE_H {
            if tool == Tool::Select {
                let mut st = state.borrow_mut();
                st.selected_index = None;
                st.selected_indices.clear();
                drop(st);
                c.queue_draw();
            }
            return Propagation::Proceed;
        }

        if event.event_type() == gdk::EventType::DoubleButtonPress {
            let mut st = state.borrow_mut();
            if let Some(idx) = crate::models::select::hit_test_component(&st.current_page_data, px, py) {
                if let ComponentPayload::RichText(ref block) = st.current_page_data.components[idx] {
                    
                    let existing_text = block.spans.iter().map(|s| s.text.clone()).collect::<String>();
                    let existing_style = block.spans.first().map(|s| s.style.clone()).unwrap_or_default();
                    
                    drop(st); 
                    
                    
                    if let Some((new_text, new_style)) = crate::models::textbox::show_text_input_dialog(&w, &existing_style, &existing_text) {
                        let mut st_mut = state.borrow_mut();
                        let mut invalidate_id = None; 

                        if let ComponentPayload::RichText(ref mut b) = st_mut.current_page_data.components[idx] {
                            
                            b.spans = vec![TextSpan { text: new_text, style: new_style }];
                            
                            
                            invalidate_id = Some(format!("txt_{}", b.id_temporaneo));
                        }

                        
                        if let Some(id) = invalidate_id {
                            st_mut.image_cache.borrow_mut().remove(&id);
                        }
                    }
                    return Propagation::Stop; 
                }
            }
        }

        match tool 
        {
            
            Tool::Pen => {
                let color = s.borrow().current_color.clone();
                let width = s.borrow().current_width;
                let mut st = s.borrow_mut();
                st.is_drawing = true;
                st.current_stroke = Some(Stroke { points: vec![(px, py)], color, width });
            }
            Tool::Eraser => {
                let mut st = s.borrow_mut();
                st.save_snapshot();
                st.is_drawing = true;
                if let Some(idx) = crate::models::select::hit_test_component(&st.current_page_data, px, py) {
                    if matches!(st.current_page_data.components[idx], ComponentPayload::PenStroke(_)) {
                        st.current_page_data.components.remove(idx);
                        c.queue_draw();
                    }
                }
            }

            
            Tool::Text => {
                
                
                let default_style = s.borrow().current_text_style.clone();
                
                if let Some((text, style)) = show_text_input_dialog(&w, &default_style, "") {
                    let mut st = s.borrow_mut();
                    st.current_text_style = style.clone();
                    st.text_id_counter += 1;
                    let id = format!("txt_{}", st.text_id_counter);

                    
                    let est_width = (text.len() as f64 * style.size * 0.55).min(400.0).max(50.0);

                    let block = RichTextBlock {
                        id_temporaneo: id,
                        x: px,
                        y: py,
                        width: est_width,
                        spans: vec![TextSpan { text, style }],
                    };
                    st.commit_component(ComponentPayload::RichText(block));
                    let title = st.window_title();
                    drop(st);
                    w.set_title(&title);
                }
            }

            
            Tool::Select => {
                let mut st = s.borrow_mut();
                let mut handled = false;

                
                if st.selected_indices.len() == 1 {
                    let sel_idx = st.selected_indices[0];
                    if let Some(comp) = st.current_page_data.components.get(sel_idx).cloned() {
                        if let Some(handle) = hit_test_resize_handle(&comp, px, py) {
                            let (orig_x, orig_y, orig_w, orig_h) = match comp {
                                ComponentPayload::Image(ref b) => (b.x, b.y, if b.width > 0.0 { b.width } else { 150.0 }, if b.height > 0.0 { b.height } else { 100.0 }),
                                ComponentPayload::RichText(ref b) => {
                                    let (mnx, mxx, mny, mxy) = b.approx_bbox();
                                    (mnx, mny, mxx - mnx, mxy - mny)
                                }
                                _ => (0.0, 0.0, 0.0, 0.0),
                            };
                            st.drag_mode = DragMode::Resize {
                                handle, orig_x, orig_y, orig_w, orig_h, start_px: px, start_py: py,
                            };
                            handled = true;
                        }
                    }
                }

                if !handled {
                    let hit = hit_test_component(&st.current_page_data, px, py);
                    if let Some(idx) = hit {
                        if !st.selected_indices.contains(&idx) {
                            st.selected_indices = vec![idx];
                        }
                        
                        
                        st.selected_index = if st.selected_indices.len() == 1 {
                            Some(st.selected_indices[0])
                        } else {
                            None
                        };

                        let mut orig_positions = Vec::new();
                        for &s_idx in &st.selected_indices {
                            if let Some(comp) = st.current_page_data.components.get(s_idx) {
                                let (ox, oy) = match comp {
                                    ComponentPayload::PenStroke(s) | ComponentPayload::EraserStroke(s) => s.points.first().copied().unwrap_or((0.0, 0.0)),
                                    ComponentPayload::RichText(b) => (b.x, b.y),
                                    ComponentPayload::Image(b)    => (b.x, b.y),
                                    ComponentPayload::Shape(b) => (b.x1, b.y1),
                                };
                                orig_positions.push((s_idx, ox, oy));
                            }
                        }
                        st.drag_mode = DragMode::Move { start_px: px, start_py: py, orig_positions };
                    } else {
                        st.selected_indices.clear();
                        st.selected_index = None; 
                        st.drag_mode = DragMode::Marquee { start_px: px, start_py: py, current_px: px, current_py: py };
                    }
                }

                if matches!(st.drag_mode, DragMode::Move { .. } | DragMode::Resize { .. }) {
                    st.save_snapshot();
                }
            }
            Tool::Shape(kind) => {
                let color = s.borrow().current_color.clone();
                let width = s.borrow().current_width;
                let mut st = s.borrow_mut();
                st.is_drawing = true;
                st.current_shape = Some(ShapeBlock { kind: kind.clone(), x1: px, y1: py, x2: px, y2: py, color, width });
            }
        }
    
        c.queue_draw();
        Propagation::Proceed
        }));
    }

    
    {
        let s = state.clone();
        let c = canvas.clone();
        canvas.connect_motion_notify_event(move |_, event| {
        let tool = s.borrow().active_tool.clone();

        if tool == Tool::Select {
            let (mx, my) = event.position();
            let (ox, oy) = s.borrow().page_origin;
            let zoom = s.borrow().zoom;
            let px = ((mx - ox) / zoom).clamp(0.0, PAGE_W);
            let py = ((my - oy) / zoom).clamp(0.0, PAGE_H);

            let drag_mode = s.borrow().drag_mode.clone();
            match drag_mode {
                DragMode::Move { start_px, start_py, ref orig_positions } => {
                    let dx = px - start_px;
                    let dy = py - start_py;
                    let mut st = s.borrow_mut();
                    
                    for &(idx, orig_x, orig_y) in orig_positions {
                        let new_x = orig_x + dx;
                        let new_y = orig_y + dy;
                        if let Some(comp) = st.current_page_data.components.get_mut(idx) {
                            match comp {
                                ComponentPayload::Shape(b) => {
                                    let shift_x = new_x - b.x1;
                                    let shift_y = new_y - b.y1;
                                    b.x1 += shift_x; b.x2 += shift_x;
                                    b.y1 += shift_y; b.y2 += shift_y;
                                }
                                ComponentPayload::PenStroke(stroke) | ComponentPayload::EraserStroke(stroke) => {
                                    let first = stroke.points.first().copied().unwrap_or((0.0, 0.0));
                                    let shift_x = new_x - first.0;
                                    let shift_y = new_y - first.1;
                                    for pt in stroke.points.iter_mut() {
                                        pt.0 += shift_x; pt.1 += shift_y;
                                    }
                                }
                                ComponentPayload::RichText(b) => { b.x = new_x; b.y = new_y; }
                                ComponentPayload::Image(b) => { b.x = new_x; b.y = new_y; }
                            }
                        }
                    }
                    c.queue_draw();
                }
                DragMode::Marquee { start_px, start_py, .. } => {
                    let mut st = s.borrow_mut();
                    st.drag_mode = DragMode::Marquee { start_px, start_py, current_px: px, current_py: py };
                    st.selected_indices = hit_test_marquee(&st.current_page_data, start_px, start_py, px, py);
                    
                    
                    st.selected_index = if st.selected_indices.len() == 1 {
                        Some(st.selected_indices[0])
                    } else {
                        None
                    };
                    c.queue_draw();
                }
                DragMode::Resize { ref handle, orig_x, orig_y, orig_w, orig_h, start_px, start_py } => {
                    let dx = px - start_px;
                    let dy = py - start_py;
                    let sel_idx = s.borrow().selected_indices.first().copied();
                    if let Some(idx) = sel_idx {
                        let mut st = s.borrow_mut();
                        if let Some(comp) = st.current_page_data.components.get_mut(idx) {
                            let min_size = 20.0;
                            match comp {
                                ComponentPayload::Image(block) => {
                                    match handle {
                                        ResizeHandle::BottomRight => {
                                            block.width  = (orig_w + dx).max(min_size);
                                            block.height = (orig_h + dy).max(min_size);
                                        }
                                        ResizeHandle::BottomLeft => {
                                            let new_w = (orig_w - dx).max(min_size);
                                            block.x     = orig_x + (orig_w - new_w);
                                            block.width  = new_w;
                                            block.height = (orig_h + dy).max(min_size);
                                        }
                                        ResizeHandle::TopRight => {
                                            let new_h = (orig_h - dy).max(min_size);
                                            block.y      = orig_y + (orig_h - new_h);
                                            block.width  = (orig_w + dx).max(min_size);
                                            block.height = new_h;
                                        }
                                        ResizeHandle::TopLeft => {
                                            let new_w = (orig_w - dx).max(min_size);
                                            let new_h = (orig_h - dy).max(min_size);
                                            block.x      = orig_x + (orig_w - new_w);
                                            block.y      = orig_y + (orig_h - new_h);
                                            block.width  = new_w;
                                            block.height = new_h;
                                        }
                                    }
                                }
                                ComponentPayload::RichText(block) => {
                                    
                                    
                                    match handle {
                                        ResizeHandle::BottomRight | ResizeHandle::TopRight => {
                                            block.width = (orig_w + dx).max(min_size);
                                        }
                                        ResizeHandle::BottomLeft | ResizeHandle::TopLeft => {
                                            let new_w = (orig_w - dx).max(min_size);
                                            block.x = orig_x + (orig_w - new_w);
                                            block.width = new_w;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    c.queue_draw();
                }
                DragMode::None => {}
            }
            return Propagation::Proceed;
        }

        if s.borrow().is_drawing {
            let (mx, my) = event.position();
            let (ox, oy) = s.borrow().page_origin;
            let zoom = s.borrow().zoom;
            let px = ((mx - ox) / zoom).clamp(0.0, PAGE_W);
            let py = ((my - oy) / zoom).clamp(0.0, PAGE_H);
            
            if tool == Tool::Pen {
                if let Some(ref mut stroke) = s.borrow_mut().current_stroke {
                    stroke.points.push((px, py));
                }
                c.queue_draw();
            } else if tool == Tool::Eraser {
                let mut st = s.borrow_mut();
                if let Some(idx) = crate::models::select::hit_test_component(&st.current_page_data, px, py) {
                    if matches!(st.current_page_data.components[idx], ComponentPayload::PenStroke(_)) {
                        st.current_page_data.components.remove(idx);
                        c.queue_draw();
                    }
                }
            } else if let Tool::Shape(_) = tool {
                if let Some(ref mut shape) = s.borrow_mut().current_shape {
                    shape.x2 = px;
                    shape.y2 = py;
                }
                c.queue_draw();
            }
        }
        Propagation::Proceed
    });
    }

    
    {
        let s = state.clone();
        let c = canvas.clone();
        let w = window.clone();
        canvas.connect_button_release_event(move |_, event| {
            let button = event.button();
            
            let btn_trigger = EventTrigger::Mouse(button);
            
            
            let should_restore = {
                let st = s.borrow();
                st.active_temp_trigger.as_ref() == Some(&btn_trigger)
            };
            
            if should_restore {
                let (prev_tool, cb) = {
                    let st = s.borrow();
                    (st.previous_tool.clone(), st.update_toolbar_ui.clone())
                };
                
                let mut st_mut = s.borrow_mut();
                if let Some(p) = &prev_tool {
                    st_mut.active_tool = p.clone();
                }
                st_mut.previous_tool = None;
                st_mut.active_temp_trigger = None;
                drop(st_mut);
                
                if let Some(p) = prev_tool {
                    if let Some(f) = cb { f(&p); }
                }
                c.queue_draw();
                
                if button != 1 {
                    return Propagation::Stop;
                }
            }
            
            if button != 1 { return Propagation::Proceed; }

            let tool = s.borrow().active_tool.clone();

            if tool == Tool::Select {
                let drag_mode = s.borrow().drag_mode.clone();
                match drag_mode {
                    DragMode::Move { orig_positions, .. } => {
                        let mut st = s.borrow_mut();
                        let mut out_of_bounds = false;

                        
                        for &idx in &st.selected_indices {
                            if let Some(payload) = st.current_page_data.components.get(idx) {
                                if let Some((bx, by, bw, bh)) = component_bbox(payload) {
                                    if bx < 0.0 || by < 0.0 || bx + bw > PAGE_W || by + bh > PAGE_H {
                                        out_of_bounds = true;
                                        break;
                                    }
                                }
                            }
                        }

                        if out_of_bounds {
                            
                            for &(idx, orig_x, orig_y) in &orig_positions {
                                if let Some(comp) = st.current_page_data.components.get_mut(idx) {
                                    match comp {
                                        ComponentPayload::Shape(b) => {
                                            let shift_x = orig_x - b.x1;
                                            let shift_y = orig_y - b.y1;
                                            b.x1 += shift_x; b.x2 += shift_x;
                                            b.y1 += shift_y; b.y2 += shift_y;
                                        }
                                        ComponentPayload::PenStroke(stroke) | ComponentPayload::EraserStroke(stroke) => {
                                            let first = stroke.points.first().copied().unwrap_or((0.0, 0.0));
                                            let shift_x = orig_x - first.0;
                                            let shift_y = orig_y - first.1;
                                            for pt in stroke.points.iter_mut() { pt.0 += shift_x; pt.1 += shift_y; }
                                        }
                                        ComponentPayload::RichText(b) => { b.x = orig_x; b.y = orig_y; }
                                        ComponentPayload::Image(b) => { b.x = orig_x; b.y = orig_y; }
                                    }
                                }
                            }

                            st.undo_stack.pop();
                            st.drag_mode = DragMode::None;
                            drop(st);
                            c.queue_draw();

                            
                            let alert = gtk::MessageDialog::new(
                                Some(&w), gtk::DialogFlags::MODAL, gtk::MessageType::Warning, gtk::ButtonsType::Ok,
                                "Non puoi spostare elementi fuori dai bordi della pagina!",
                            );
                            alert.run();
                            unsafe { alert.destroy(); }
                            return Propagation::Proceed;
                        }

                        
                        if let Some(conn) = &st.db {
                            let blob = encode_payload_list(&st.current_page_data.components);
                            
                            
                            let _ = conn.execute(
                                "DELETE FROM component_rtree WHERE id IN (SELECT id FROM active_components WHERE page_id = ?1)",
                                rusqlite::params![st.current_page_id]
                            );
                            
                            let _ = conn.execute(
                                "DELETE FROM active_components WHERE page_id = ?1",
                                rusqlite::params![st.current_page_id]
                            );
                            
                            let _ = conn.execute(
                                "UPDATE base_layers SET baked_blob = ?1 WHERE page_id = ?2",
                                rusqlite::params![blob, st.current_page_id]
                            );
                        }

                        st.is_modified = true;
                        
                        
                        

                        let title = st.window_title();
                        st.drag_mode = DragMode::None;
                        drop(st);
                        w.set_title(&title);
                        c.queue_draw();
                    }
                    DragMode::Resize { .. } => {
                        let mut st = s.borrow_mut();
                        
                        
                        if let Some(conn) = &st.db {
                            let blob = encode_payload_list(&st.current_page_data.components);
                            
                            let _ = conn.execute(
                                "DELETE FROM component_rtree WHERE id IN (SELECT id FROM active_components WHERE page_id = ?1)",
                                rusqlite::params![st.current_page_id]
                            );
                            let _ = conn.execute(
                                "DELETE FROM active_components WHERE page_id = ?1",
                                rusqlite::params![st.current_page_id]
                            );
                            let _ = conn.execute(
                                "UPDATE base_layers SET baked_blob = ?1 WHERE page_id = ?2",
                                rusqlite::params![blob, st.current_page_id]
                            );
                        }
                        
                        st.is_modified = true;
                        
                        

                        let title = st.window_title();
                        st.drag_mode = DragMode::None;
                        drop(st);
                        w.set_title(&title);
                        c.queue_draw();
                    }
                    DragMode::Marquee { .. } => {
                        s.borrow_mut().drag_mode = DragMode::None;
                        c.queue_draw();
                    }
                    DragMode::None => {}
                }
                return Propagation::Proceed;
        }

        
        
        {
            let mut st = s.borrow_mut();
            st.is_drawing = false;

            if tool == Tool::Pen {
                if let Some(stroke) = st.current_stroke.take() {
                    if stroke.points.len() >= 2 {
                        st.commit_component(ComponentPayload::PenStroke(stroke));
                        let title = st.window_title();
                        drop(st);
                        w.set_title(&title);
                        c.queue_draw();
                        return Propagation::Proceed;
                    }
                }
            } else if tool == Tool::Eraser {
                
                
                if let Some(conn) = &st.db {
                    let blob = encode_payload_list(&st.current_page_data.components);
                    let _ = conn.execute(
                        "DELETE FROM component_rtree WHERE id IN (SELECT id FROM active_components WHERE page_id = ?1)",
                        rusqlite::params![st.current_page_id]
                    );
                    let _ = conn.execute(
                        "DELETE FROM active_components WHERE page_id = ?1",
                        rusqlite::params![st.current_page_id]
                    );
                    let _ = conn.execute(
                        "UPDATE base_layers SET baked_blob = ?1 WHERE page_id = ?2",
                        rusqlite::params![blob, st.current_page_id]
                    );
                }
                st.is_modified = true;
                
                

                let title = st.window_title();
                drop(st);
                w.set_title(&title);
                c.queue_draw();
                return Propagation::Proceed;
            } else if let Tool::Shape(_) = tool {
                if let Some(shape) = st.current_shape.take() {
                    if (shape.x2 - shape.x1).abs() > 2.0 || (shape.y2 - shape.y1).abs() > 2.0 {
                        st.commit_component(ComponentPayload::Shape(shape));
                        let title = st.window_title();
                        drop(st);
                        w.set_title(&title);
                        c.queue_draw();
                        return Propagation::Proceed;
                    }
                }
            }

            drop(st);
            c.queue_draw();
        }

    Propagation::Proceed
    });
    }

    
    {
        let s = state.clone();
        let c = canvas.clone();
        let sp = spin_page.clone();
        let lt = lbl_tot.clone();
        
        canvas.connect_scroll_event(move |canvas_widget, event| {
            let (_, delta_y) = event.scroll_deltas().unwrap_or((0.0, 0.0));
            let delta_y = if delta_y == 0.0 {
                match event.direction() {
                    gdk::ScrollDirection::Down => 1.0,
                    gdk::ScrollDirection::Up   => -1.0,
                    _                          => 0.0,
                }
            } else {
                delta_y
            };

            if delta_y == 0.0 { return Propagation::Proceed; }

            let zoom = s.borrow().zoom;
            let (cur, count) = {
                let st = s.borrow();
                (st.current_page, st.page_count)
            };

            
            let mut vadj = None;
            let mut current_parent = canvas_widget.parent();
            while let Some(widget) = current_parent {
                if let Ok(sw) = widget.clone().downcast::<gtk::ScrolledWindow>() {
                    vadj = Some(sw.vadjustment());
                    break;
                }
                current_parent = widget.parent();
            }

            if let Some(adj) = vadj {
                let at_bottom = adj.value() + adj.page_size() >= adj.upper() - 1.0;
                let at_top = adj.value() <= adj.lower() + 1.0;

                
                if zoom <= 1.0 || (delta_y > 0.0 && at_bottom) {
                    if delta_y > 0.0 && cur + 1 < count {
                        let _ = s.borrow_mut().switch_to_page(cur + 1);
                        { let st = s.borrow(); sp.set_range(1.0, st.page_count as f64); sp.set_value((st.current_page + 1) as f64); lt.set_text(&format!("di {}", st.page_count)); }
                        adj.set_value(adj.lower()); 
                        c.queue_draw();
                        return Propagation::Stop;
                    }
                }
                
                
                if zoom <= 1.0 || (delta_y < 0.0 && at_top) {
                    if delta_y < 0.0 && cur > 0 {
                        let _ = s.borrow_mut().switch_to_page(cur - 1);
                        { let st = s.borrow(); sp.set_range(1.0, st.page_count as f64); sp.set_value((st.current_page + 1) as f64); lt.set_text(&format!("di {}", st.page_count)); }
                        adj.set_value(adj.upper() - adj.page_size()); 
                        c.queue_draw();
                        return Propagation::Stop;
                    }
                }
            }

            
            
            Propagation::Proceed
        });
    }

}