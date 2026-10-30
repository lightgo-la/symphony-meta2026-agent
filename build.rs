fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        if let Err(e) = res.compile() {
            eprintln!("icon resource skipped: {e}");
        }
    }
}
