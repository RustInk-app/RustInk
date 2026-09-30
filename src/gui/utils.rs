use crate::models::color::*;
use gtk::prelude::*;
use gtk::{CssProvider};
use gtk::gdk;
use glib::Propagation;
use glib::clone;
use std::cell::RefCell;
use std::collections::HashSet;

pub fn show_loading_dialog(parent: &gtk::Window, message: &str) -> gtk::Dialog {
    let dialog = gtk::Dialog::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_deletable(false);
    dialog.set_resizable(false);
    dialog.set_title("rustInk");

    let content = dialog.content_area();
    content.set_spacing(12);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(16);
    content.set_margin_bottom(16);

    let lbl = gtk::Label::new(Some(message));
    lbl.set_halign(gtk::Align::Start);
    content.pack_start(&lbl, false, false, 0);

    let bar = gtk::ProgressBar::new();
    bar.set_pulse_step(0.08);
    bar.set_show_text(false);
    content.pack_start(&bar, false, false, 0);

    dialog.show_all();

    let bar_clone = bar.clone();
    let dialog_weak = dialog.downgrade();
    glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
        if dialog_weak.upgrade().is_some() {
            bar_clone.pulse();
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });

    dialog
}

thread_local! {
    static WIRED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}


pub fn desk_rgb() -> (f64, f64, f64) { (0.078, 0.067, 0.063) }

fn icon_fg() -> &'static str { "#ffffff" }
fn on_accent() -> &'static str { "#ffffff" }


pub fn load_css() {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(true);
    }
    let css = format!("{}\n{}", include_str!("../ui/palette.css"), include_str!("../ui/style.css"));
    let provider = CssProvider::new();
    if let Err(e) = provider.load_from_data(css.as_bytes()) {
        eprintln!("Warning: could not load CSS: {e}");
    }
    if let Some(screen) = gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(&screen, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}


const ICON_MAP: &[(&str, &str)] = &[
    ("btn_save", "document-save.svg"), ("btn_open", "document-open.svg"),
    ("btn_tool_select", "select-rect.svg"), ("btn_tool_pen", "tool-pencil.svg"),
    ("btn_tool_eraser", "tool-eraser.svg"), ("btn_tool_text", "tool-text.svg"),
    ("btn_tool_shape", "tool-shape.svg"), ("btn_stroke_menu", "stroke.svg"),
    ("btn_thin", "thickness-fine.svg"), ("btn_med", "thickness-medium.svg"), ("btn_thick", "thickness-thick.svg"),
    ("btn_page_menu", "page-menu.svg"), ("btn_add_page", "page-add.svg"),
    ("btn_del_page", "page-delete.svg"), ("btn_bookmark", "bookmark.svg"),
    ("btn_hide_dock", "dock-hide.svg"), ("btn_show_dock", "dock-show.svg"),
    ("btn_zoom_out", "zoom-out.svg"), ("btn_zoom_in", "zoom-in.svg"),
];

fn set_btn_icon(btn: &gtk::Button, file: &str, active: bool) {
    
    let color = if active { on_accent() } else { icon_fg() };
    btn.set_image(Some(&load_icon_colored(file, color)));
    btn.set_always_show_image(true);
}

pub fn apply_icons(builder: &gtk::Builder) {
    for &(id, file) in ICON_MAP {
        let Some(btn) = builder.object::<gtk::Button>(id) else {
            eprintln!("[icons] widget '{id}' non trovato in menu.glade");
            continue;
        };
        let active = btn.downcast_ref::<gtk::ToggleButton>().map(|t| t.is_active()).unwrap_or(false);
        set_btn_icon(&btn, file, active);
        if let Some(tb) = btn.downcast_ref::<gtk::ToggleButton>() {
            if WIRED.with(|w| w.borrow_mut().insert(id.to_string())) {
                let file = file.to_string();
                tb.connect_toggled(move |t| set_btn_icon(t.upcast_ref::<gtk::Button>(), &file, t.is_active()));
            }
        }
    }
}



pub fn setup_chrome(builder: &gtk::Builder, _canvas: &gtk::DrawingArea) {
    apply_icons(builder);

    if let Some(logo) = builder.object::<gtk::Image>("header_logo") {
        if let Ok(pb) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(get_icon_path("rustInk_logo.png"), 30, 34, true) {
            logo.set_from_pixbuf(Some(&pb));
        }
    }

    
    let revealer: gtk::Revealer = builder.object("dock_revealer").expect("dock_revealer not found");
    let btn_hide: gtk::Button = builder.object("btn_hide_dock").expect("btn_hide_dock not found");
    let btn_show: gtk::Button = builder.object("btn_show_dock").expect("btn_show_dock not found");
    let view_toolbar: gtk::CheckMenuItem = builder.object("view_toolbar_option").expect("view_toolbar_option not found");
    view_toolbar.set_active(true);
    view_toolbar.connect_toggled(clone!(@weak revealer, @weak btn_show => move |item| {
        revealer.set_reveal_child(item.is_active());
        btn_show.set_visible(!item.is_active());
    }));
    btn_hide.connect_clicked(clone!(@weak view_toolbar => move |_| view_toolbar.set_active(false)));
    btn_show.connect_clicked(clone!(@weak view_toolbar => move |_| view_toolbar.set_active(true)));

    
    let zoom_adj: gtk::Adjustment = builder.object("zoom_adj").expect("zoom_adj not found");
    let btn_zoom_in: gtk::Button = builder.object("btn_zoom_in").expect("btn_zoom_in not found");
    let btn_zoom_out: gtk::Button = builder.object("btn_zoom_out").expect("btn_zoom_out not found");
    btn_zoom_in.connect_clicked(clone!(@weak zoom_adj => move |_| {
        zoom_adj.set_value((zoom_adj.value() + 0.25).min(zoom_adj.upper()));
    }));
    btn_zoom_out.connect_clicked(clone!(@weak zoom_adj => move |_| {
        zoom_adj.set_value((zoom_adj.value() - 0.25).max(zoom_adj.lower()));
    }));
}

use std::path::PathBuf;

pub fn get_icon_path(filename: &str) -> PathBuf {
    let mut exe_path = std::env::current_exe().unwrap_or_default();
    exe_path.pop(); 

    
    let mut path = exe_path.clone();
    path.push("icons");
    path.push(filename);

    
    if !path.exists() {
        path = exe_path.clone();
        path.pop(); 
        path.push("Resources");
        path.push("src");
        path.push("ui");
        path.push("icons");
        path.push(filename);
    }

    
    if !path.exists() {
        path = PathBuf::from(format!("src/ui/icons/{}", filename));
    }

    path
}

pub fn load_icon(filename: &str) -> gtk::Image { load_icon_colored(filename, icon_fg()) }

pub fn load_icon_colored(filename: &str, color: &str) -> gtk::Image {
    let path = get_icon_path(filename);
    
    let pixbuf = std::fs::read_to_string(&path).ok().and_then(|svg| {
        let svg = svg.replace("currentColor", color);
        let bytes = gtk::glib::Bytes::from_owned(svg.into_bytes());
        let stream = gtk::gio::MemoryInputStream::from_bytes(&bytes);
        gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale(&stream, 22, 22, true, None::<&gtk::gio::Cancellable>).ok()
    });
    match pixbuf {
        Some(pb) => gtk::Image::from_pixbuf(Some(&pb)),
        None => {
            eprintln!("Icon not found: {:?}", path);
            gtk::Image::from_icon_name(Some("image-missing"), gtk::IconSize::Menu)
        }
    }
}

pub fn make_color_button(color: &Color, label: &str) -> gtk::ToggleButton {
    let btn = gtk::ToggleButton::new();
    btn.set_tooltip_text(Some(label));
    btn.set_focus_on_click(false);
    btn.style_context().add_class("ink-swatch");   
    let area = gtk::DrawingArea::new();
    area.set_size_request(18, 18);
    let c = color.clone();
    area.connect_draw(move |_w, cr| {
        cr.set_source_rgb(c.r, c.g, c.b);
        cr.arc(9.0, 9.0, 9.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
        glib::Propagation::Proceed
    });
    btn.add(&area);
    btn
}
