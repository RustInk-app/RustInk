use rustInk::gui::build_ui;

use gtk::prelude::*;
use gtk::{Application};

fn main() {

    let app = Application::builder()
        .application_id("com.github.rustInk.app")
        
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(build_ui);
    let exit_code = app.run();
    std::process::exit(exit_code.into());
}