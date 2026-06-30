use crate::models::color::*;
use crate::models::image::*;
use crate::models::page::*;
use crate::models::select::*;
use crate::models::stroke::*;
use crate::models::textbox::*;

use crate::translate_xournal::*;

use crate::save_handler::autosave;
use crate::save_handler::db::*;

use gtk::cairo;
use gtk::gdk;
use gtk::prelude::*;
use gtk::{Application, Builder, CssProvider, Window};

use glib::Propagation;
use glib::clone;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq)]
pub enum EventTrigger {
    Mouse(u32),
    Key(String),
}

pub struct AppState {
    pub db: Option<rusqlite::Connection>,
    pub db_tmp_path: Option<PathBuf>,
    pub bundle_path: Option<PathBuf>,

    pub is_modified: bool,
    pub lock_path: Option<PathBuf>,

    pub page_count: usize,
    pub current_page: usize,
    pub current_page_id: i64,
    pub current_page_data: PageData,

    pub undo_stack: Vec<i64>,
    pub redo_stack: Vec<i64>,

    pub current_color: Color,
    pub current_width: f64,
    pub is_drawing: bool,
    pub current_stroke: Option<Stroke>,
    pub active_tool: Tool,
    pub page_origin: (f64, f64),

    pub current_text_style: TextStyle,
    pub current_text_width: f64,
    pub text_id_counter: u64,

    pub zoom: f64,
    pub scroll_offset_y: f64,

    pub selected_index: Option<usize>,
    pub drag_mode: DragMode,
    pub image_cache: RefCell<std::collections::HashMap<String, cairo::ImageSurface>>,
    pub selected_indices: Vec<usize>,
    pub paper_background: PaperBackground,
    
    // Nuovi campi per le preferenze e hold-to-switch
    pub pref_trigger_1: Option<EventTrigger>,
    pub pref_tool_1: Option<Tool>,
    pub pref_trigger_2: Option<EventTrigger>,
    pub pref_tool_2: Option<Tool>,
    pub previous_tool: Option<Tool>,
    pub active_temp_trigger: Option<EventTrigger>,
    pub update_toolbar_ui: Option<Rc<dyn Fn(&Tool)>>,

    pub current_shape: Option<ShapeBlock>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            db: None,
            db_tmp_path: None,
            bundle_path: None,
            is_modified: false,
            lock_path: None,
            page_count: 1,
            current_page: 0,
            current_page_id: 1,
            current_page_data: PageData::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_color: Color::black(),
            current_width: STROKE_MEDIUM,
            is_drawing: false,
            current_stroke: None,
            active_tool: Tool::Pen,
            page_origin: (0.0, 0.0),
            current_text_style: TextStyle::default(),
            current_text_width: 300.0,
            text_id_counter: 0,
            zoom: 1.0,
            scroll_offset_y: 0.0,
            selected_index: None,
            drag_mode: DragMode::None,
            image_cache: RefCell::new(std::collections::HashMap::new()),
            selected_indices: Vec::new(),
            paper_background: PaperBackground::Grid,
            
            pref_trigger_1: None,
            pref_tool_1: None,
            pref_trigger_2: None,
            pref_tool_2: None,
            previous_tool: None,
            active_temp_trigger: None,
            update_toolbar_ui: None,

            current_shape: None,
        }
    }

    pub fn init_new_document(&mut self) -> rusqlite::Result<()> {
        
        let tmp = autosave::temp_db_dir();

        let conn = rusqlite::Connection::open(&tmp)?;

        init_schema(&conn)?;

        conn.execute("INSERT INTO pages (display_order) VALUES (0)", [])?;
        let page_id = conn.last_insert_rowid();

        let _ =
            crate::save_handler::db::update_page_background(&conn, page_id, &PaperBackground::Grid);

        conn.execute(
            "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)",
            rusqlite::params![page_id, encode_payload_list(&[])],
        )?;

        eprintln!("[DB] Nuovo documento: path={:?}, page_id={}", tmp, page_id);

        self.page_count = 1;
        self.current_page = 0;
        self.current_page_id = page_id;
        self.current_page_data = PageData::new();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.bundle_path = None;
        self.is_modified = false;
        self.release_lock();
        self.db_tmp_path = Some(tmp);
        self.db = Some(conn);
        Ok(())
    }

    pub fn switch_to_page(&mut self, new_index: usize) -> rusqlite::Result<()> {
        if new_index == self.current_page {
            return Ok(());
        }
        if let Some(conn) = &self.db {
            let new_id = page_id_at(conn, new_index)?;
            let page = load_page(conn, new_id)?;
            self.current_page = new_index;
            self.current_page_id = new_id;
            self.paper_background = page.background.clone();
            self.current_page_data = page;

            //self.undo_stack.clear();
            //self.redo_stack.clear();
        }
        Ok(())
    }

    pub fn commit_component(&mut self, payload: ComponentPayload) {
        if let Some(conn) = &self.db {
            match append_active_component(conn, self.current_page_id, &payload) {
                Ok(new_id) => {
                    self.current_page_data.components.push(payload);
                    self.undo_stack.push(new_id);
                    self.redo_stack.clear();
                    self.is_modified = true;
                    eprintln!(
                        "[DB] append component id={new_id}, undo_stack={}",
                        self.undo_stack.len()
                    );
                }
                Err(e) => eprintln!("[DB] ERRORE append_active_component: {e}"),
            }
        }
    }

    pub fn undo(&mut self) {
        if let Some(id) = self.undo_stack.pop() {
            if let Some(conn) = &self.db {
                if let Err(e) = toggle_active_state(conn, id, false) {
                    eprintln!("[DB] ERRORE undo toggle: {e}");
                    self.undo_stack.push(id);
                    return;
                }
            }
            self.redo_stack.push(id);
            self.reload_current_page();
            self.is_modified = true;
            eprintln!("[DB] Undo id={id}");
        }
    }

    pub fn redo(&mut self) {
        if let Some(id) = self.redo_stack.pop() {
            if let Some(conn) = &self.db {
                if let Err(e) = toggle_active_state(conn, id, true) {
                    eprintln!("[DB] ERRORE redo toggle: {e}");
                    self.redo_stack.push(id);
                    return;
                }
            }
            self.undo_stack.push(id);
            self.reload_current_page();
            self.is_modified = true;
            eprintln!("[DB] Redo id={id}");
        }
    }

    pub fn reload_current_page(&mut self) {
        if let Some(conn) = &self.db {
            match load_page(conn, self.current_page_id) {
                Ok(pd) => self.current_page_data = pd,
                Err(e) => eprintln!("[DB] ERRORE reload_current_page: {e}"),
            }
        }
    }

    pub fn acquire_lock(&mut self, bundle: &PathBuf) -> bool {
        self.release_lock();
        let lock = bundle.with_extension("rastin.lock");
        if lock.exists() {
            return false;
        }
        match std::fs::write(&lock, std::process::id().to_string()) {
            Ok(_) => {
                self.lock_path = Some(lock);
                true
            }
            Err(e) => {
                eprintln!("[LOCK] impossibile creare il lock: {e}");
                true
            }
        }
    }

    pub fn release_lock(&mut self) {
        if let Some(lp) = self.lock_path.take() {
            let _ = std::fs::remove_file(&lp);
        }
    }

    pub fn window_title(&self) -> String {
        let base = match &self.bundle_path {
            Some(p) => p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown Document")
                .to_string(),
            None => "Unknown Document".to_string(),
        };
        if self.is_modified {
            format!("{}*", base)
        } else {
            base
        }
    }
}
