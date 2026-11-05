use std::path::PathBuf;

fn main() {
    let payload = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("payload")
        .join("app.zip");
    if !payload.exists() {
        std::fs::create_dir_all(payload.parent().expect("payload dir")).expect("create payload dir");
        let file = std::fs::File::create(&payload).expect("create placeholder zip");
        let w = zip::ZipWriter::new(file);
        w.finish().expect("finish placeholder zip");
    }
    let manifest = payload.with_file_name("resources.json");
    if !manifest.exists() {
        std::fs::write(&manifest, "{}").expect("create placeholder manifest");
    }
    println!("cargo:rerun-if-changed=payload/app.zip");
    println!("cargo:rerun-if-changed=payload/resources.json");
    tauri_build::build()
}
