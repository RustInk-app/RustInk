fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/ui/icons/rustInk_logo.ico");
        res.compile().unwrap();
    }
}