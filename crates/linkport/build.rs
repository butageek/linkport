// Ensures the rust-embed source folder exists so the crate compiles before
// the frontend has been built at least once, and embeds the app icon into
// the Windows exes (Default-apps / Start-menu icons, Explorer file icon).

use std::fs;
use std::path::Path;

fn main() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = manifest.join("../../frontend/out");
    if !dir.join("index.html").exists() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("index.html"), PLACEHOLDER);
        println!(
            "cargo:warning=created placeholder portal at {} (build the real UI with `cd frontend && npm run build`)",
            dir.display()
        );
    }
    println!("cargo:rerun-if-changed=build.rs");

    // Icon resource for the Windows binaries. windres comes with mingw-w64.
    if std::env::var("TARGET")
        .map(|t| t.contains("windows"))
        .unwrap_or(false)
    {
        let icon = manifest.join("../../resources/icon.ico");
        println!("cargo:rerun-if-changed={}", icon.display());
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id(&icon.display().to_string(), "1");
        if let Err(e) = res.compile() {
            panic!("failed to embed icon (is mingw-w64 windres installed?): {e}");
        }
    }
}

const PLACEHOLDER: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Linkport</title></head>
<body style="font-family: system-ui, sans-serif; padding: 3rem; max-width: 40rem;">
<h1>Linkport portal placeholder</h1>
<p>The frontend has not been built yet. Run
<code>cd frontend && npm install && npm run build</code>, then rebuild this
binary.</p>
</body></html>"#;
