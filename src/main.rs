mod models;
mod gui;
mod save_handler;
mod translate_xournal;
mod export; 

use crate::models::color;
use crate::models::textbox;
use crate::models::image;
use crate::models::page;
use crate::models::stroke;

use crate::gui::build_ui;
use crate::save_handler::autosave::*;

use gtk::prelude::*;
use gtk::{Application};

fn main() {

    let app = Application::builder()
        .application_id("com.github.rastin.app")
        // Allow multiple instance
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(build_ui);
    let exit_code = app.run();
    std::process::exit(exit_code.into());
}