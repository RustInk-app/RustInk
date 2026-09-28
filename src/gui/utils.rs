use crate::models::color::*;
use gtk::prelude::*;
use gtk::{CssProvider};
use gtk::gdk;
use glib::Propagation;

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

pub fn load_css() {
    let provider = CssProvider::new();
    let css_path = include_str!("../ui/style.css");
 
    if let Err(e) = provider.load_from_data(css_path.as_bytes()) {
        eprintln!("Warning: could not load CSS: {e}");
    }
    if let Some(screen) = gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn load_icon(filename: &str) -> gtk::Image {
    let path = format!("src/ui/icons/{}", filename);
    if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, 24, 24, true) {
        gtk::Image::from_pixbuf(Some(&pixbuf))
    } else {
        eprintln!("Icon not found: {}", path);
        gtk::Image::from_icon_name(Some("image-missing"), gtk::IconSize::Menu)
    }
}

pub fn make_color_button(color: &Color, label: &str) -> gtk::ToggleButton {
    let btn = gtk::ToggleButton::new();
    btn.set_tooltip_text(Some(label));
    btn.set_size_request(28, 28);
    let area = gtk::DrawingArea::new();
    area.set_size_request(20, 20);
    let c = color.clone();
    area.connect_draw(move |_w, cr| {
        cr.set_source_rgb(c.r, c.g, c.b);
        cr.rectangle(0.0, 0.0, 20.0, 20.0);
        let _ = cr.fill();
        cr.set_source_rgb(0.3, 0.3, 0.3);
        cr.set_line_width(1.0);
        cr.rectangle(0.5, 0.5, 19.0, 19.0);
        let _ = cr.stroke();
        glib::Propagation::Proceed
    });
    btn.add(&area);
    btn
}