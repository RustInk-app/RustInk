use crate::models::color::*;
use crate::models::page::*;
use crate::models::select::*;
use crate::models::stroke::*;
use crate::models::textbox::*;

use crate::save_handler::autosave;
use crate::save_handler::db::*;
use crate::save_handler::database_pdf_utilities::*;
use crate::save_handler::database_utilities::*;

use gtk::cairo;
use gtk::prelude::*;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::save_handler::autosave_utilities::*;

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

    pub undo_stack: Vec<Vec<ComponentPayload>>,
    pub redo_stack: Vec<Vec<ComponentPayload>>,

    pub current_color: Color,
    pub current_width: f64,
    pub is_drawing: bool,
    pub current_stroke: Option<Stroke>,
    pub active_tool: Tool,
    pub page_origin: (f64, f64),

    pub current_text_style: TextStyle,
    pub text_id_counter: u64,

    pub zoom: f64,
    

    pub selected_index: Option<usize>,
    pub drag_mode: DragMode,
    pub image_cache: RefCell<std::collections::HashMap<String, cairo::ImageSurface>>,
    pub selected_indices: Vec<usize>,
    pub paper_background: PaperBackground,
    
    
    pub pref_trigger_1: Option<EventTrigger>,
    pub pref_tool_1: Option<Tool>,
    pub pref_trigger_2: Option<EventTrigger>,
    pub pref_tool_2: Option<Tool>,
    pub previous_tool: Option<Tool>,
    pub active_temp_trigger: Option<EventTrigger>,
    pub update_toolbar_ui: Option<Rc<dyn Fn(&Tool)>>,
    
    pub update_bookmark_ui: Option<Rc<dyn Fn(bool)>>, 
    
    pub search_query: String,
    pub bookmark_trie: SearchTrieNode,
    pub bookmarked_pages: std::collections::HashSet<usize>, 
    
    pub current_shape: Option<ShapeBlock>,
    pub current_pdf_ref: Option<crate::models::page::PdfPageRef>,
    pub pdf_cache: RefCell<std::collections::HashMap<i64, poppler::Document>>,
    pub pdf_surface_cache: RefCell<std::collections::HashMap<(i64, i64, u32), cairo::ImageSurface>>,
    
    pub thumbnail_cache: RefCell<std::collections::HashMap<usize, cairo::ImageSurface>>,
    pub pending_thumbnails: RefCell<std::collections::HashSet<usize>>,

    pub thumb_req_stack: std::sync::Arc<std::sync::Mutex<Vec<(std::path::PathBuf, usize, u64)>>>,
    pub thumb_wakeup_tx: Option<std::sync::mpsc::Sender<()>>,

    pub doc_generation: u64,
    pub clipboard: Vec<crate::models::page::ComponentPayload>,
}

#[derive(Default, Debug)]
pub struct SearchTrieNode {
    children: std::collections::HashMap<char, SearchTrieNode>,
    pub pages: std::collections::HashSet<usize>,
}

impl SearchTrieNode {
    pub fn insert(&mut self, word: &str, page_idx: usize) {
        let mut node = self;
        for c in word.chars() {
            node.pages.insert(page_idx); 
            node = node.children.entry(c).or_default();
        }
        node.pages.insert(page_idx); 
    }

    pub fn search(&self, prefix: &str) -> Option<&std::collections::HashSet<usize>> {
        let mut node = self;
        for c in prefix.chars() {
            if let Some(n) = node.children.get(&c) { node = n; } 
            else { return None; }
        }
        Some(&node.pages)
    }
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
            text_id_counter: 0,
            zoom: 1.0,
            
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
            update_bookmark_ui: None,
            
            search_query: String::new(),
            bookmark_trie: SearchTrieNode::default(),
            bookmarked_pages: std::collections::HashSet::new(),
            
            current_shape: None,

            current_pdf_ref: None,

            pdf_cache: RefCell::new(std::collections::HashMap::new()),
            pdf_surface_cache: RefCell::new(std::collections::HashMap::new()),
            
            thumbnail_cache: RefCell::new(std::collections::HashMap::new()),
            pending_thumbnails: RefCell::new(std::collections::HashSet::new()),

            thumb_req_stack: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            thumb_wakeup_tx: None,
            doc_generation: 0,

            clipboard: Vec::new(),
        }
    }

    pub fn init_new_document(&mut self) -> rusqlite::Result<()> {
        
        let tmp = temp_db_dir();

        let conn = rusqlite::Connection::open(&tmp)?;

        init_schema(&conn)?;

        conn.execute("INSERT INTO pages (display_order) VALUES (0)", [])?;
        let page_id = conn.last_insert_rowid();

         let _ = update_page_background(&conn, page_id, &PaperBackground::Grid);

        conn.execute(
            "INSERT INTO base_layers (page_id, baked_blob) VALUES (?1, ?2)",
            rusqlite::params![page_id, encode_payload_list(&[])],
        )?;

        eprintln!("New document created: path={:?}, page_id={}", tmp, page_id);

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

        
        
        self.doc_generation += 1;
        self.current_pdf_ref = None;
        self.pdf_cache.borrow_mut().clear();
        self.pdf_surface_cache.borrow_mut().clear();
        self.thumbnail_cache.borrow_mut().clear();
        self.pending_thumbnails.borrow_mut().clear();
        self.image_cache.borrow_mut().clear();

        Ok(())
    }

    
    pub fn rebuild_bookmark_index(&mut self) {
        self.bookmark_trie = SearchTrieNode::default();
        self.bookmarked_pages.clear();

        if let Some(conn) = &self.db {
            
            if let Ok(mut stmt) = conn.prepare("SELECT id, display_order, bookmark_name FROM pages WHERE is_bookmarked = 1") {
                let iter = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)? as usize,
                        row.get::<_, Option<String>>(2)?
                    ))
                }).unwrap();
                
                for res in iter.flatten() {
                    let (id, idx, name_opt) = res;
                    self.bookmarked_pages.insert(idx);
                    
                    if let Some(name) = name_opt {
                        let text = name.to_lowercase();
                        
                        self.bookmark_trie.insert(&text, idx);
                        for word in text.split_whitespace() {
                            self.bookmark_trie.insert(word, idx);
                        }
                    }
                }
            }
        }
    }
    
    pub fn switch_to_page(&mut self, new_index: usize) -> rusqlite::Result<()> {
        if new_index == self.current_page { return Ok(()); }
        if let Some(conn) = &self.db {
            let new_id = page_id_at(conn, new_index)?;
            let page = load_page(conn, new_id)?;
            self.current_page = new_index;
            self.current_page_id = new_id;
            self.paper_background = page.background.clone();
            self.current_page_data = page.clone();

            
            self.current_pdf_ref = get_page_pdf_ref(conn, new_id)?
                .map(|(doc_id, page_index)| crate::models::page::PdfPageRef { doc_id, page_index });

            if let Some(pref) = self.current_pdf_ref {
                self.ensure_pdf_loaded(pref.doc_id);
            }

            if let Some(cb) = &self.update_bookmark_ui {
                cb(page.is_bookmarked);
            }
        }
        Ok(())
    }

    
    pub fn ensure_pdf_loaded(&self, doc_id: i64) {
        if self.pdf_cache.borrow().contains_key(&doc_id) {
            return;
        }
        let Some(conn) = &self.db else { return };
        let Ok(row) = get_pdf_document(conn, doc_id) else { return };

        
        let full_path = SESSION_TEMP_DIR
            .path()
            .join(&row.relative_path);

        let uri = gio::File::for_path(&full_path).uri();
        match poppler::Document::from_file(&uri, None) {
            Ok(doc) => { self.pdf_cache.borrow_mut().insert(doc_id, doc); }
            Err(e) => eprintln!("Unable to load PDF {:?}: {e}", full_path),
        }
    }

    pub fn commit_component(&mut self, payload: ComponentPayload) {
        self.save_snapshot(); 

        if let Some(conn) = &self.db {
            match append_active_component(conn, self.current_page_id, &payload) {
                Ok(_) => {
                    self.current_page_data.components.push(payload);
                    self.is_modified = true;
                    self.thumbnail_cache.borrow_mut().remove(&self.current_page);
                }
                Err(e) => eprintln!("Error in append_active_component: {e}"),
            }
        }
    }

    pub fn undo(&mut self) {
        if let Some(previous_components) = self.undo_stack.pop() {
            self.redo_stack.push(self.current_page_data.components.clone());
            self.current_page_data.components = previous_components;
            self.sync_components_to_db();
            
            
            self.selected_indices.clear();
            self.selected_index = None;
            self.drag_mode = DragMode::None;
            
            self.is_modified = true;
            self.thumbnail_cache.borrow_mut().remove(&self.current_page);
            self.reload_current_page();
            eprintln!("Restored from last file auto-saved");
        }
    }

    pub fn redo(&mut self) {
        if let Some(next_components) = self.redo_stack.pop() {
            self.undo_stack.push(self.current_page_data.components.clone());
            self.current_page_data.components = next_components;
            self.sync_components_to_db();
            
            self.selected_indices.clear();
            self.selected_index = None;
            self.drag_mode = DragMode::None;
            
            self.is_modified = true;
            self.thumbnail_cache.borrow_mut().remove(&self.current_page);
            self.reload_current_page();
            eprintln!("Restored from last file auto-saved");
        }
    }

    pub fn save_snapshot(&mut self) {
        self.undo_stack.push(self.current_page_data.components.clone());
        self.redo_stack.clear();
    }

    pub fn sync_components_to_db(&self) {
        if let Some(conn) = &self.db {
            let blob = encode_payload_list(&self.current_page_data.components);
            let _ = conn.execute(
                "DELETE FROM component_rtree WHERE id IN (SELECT id FROM active_components WHERE page_id = ?1)",
                rusqlite::params![self.current_page_id]
            );
            let _ = conn.execute(
                "DELETE FROM active_components WHERE page_id = ?1",
                rusqlite::params![self.current_page_id]
            );
            let _ = conn.execute(
                "UPDATE base_layers SET baked_blob = ?1 WHERE page_id = ?2",
                rusqlite::params![blob, self.current_page_id]
            );
        }
    }

    pub fn reload_current_page(&mut self) {
        if let Some(conn) = &self.db {
            match load_page(conn, self.current_page_id) {
                Ok(pd) => self.current_page_data = pd,
                Err(e) => eprintln!("Error in reload_current_page: {e}"),
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
                eprintln!("Unable to create scheduling lock: {e}");
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
