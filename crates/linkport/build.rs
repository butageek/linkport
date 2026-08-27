// Ensures the rust-embed source folder exists so the crate compiles before
// the frontend has been built at least once.

use std::fs;
use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frontend/out");
    if !dir.join("index.html").exists() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("index.html"), PLACEHOLDER);
        println!(
            "cargo:warning=created placeholder portal at {} (build the real UI with `cd frontend && npm run build`)",
            dir.display()
        );
    }
    println!("cargo:rerun-if-changed=build.rs");
}

const PLACEHOLDER: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Linkport</title></head>
<body style="font-family: system-ui, sans-serif; padding: 3rem; max-width: 40rem;">
<h1>Linkport portal placeholder</h1>
<p>The frontend has not been built yet. Run
<code>cd frontend && npm install && npm run build</code>, then rebuild this
binary.</p>
</body></html>"#;
