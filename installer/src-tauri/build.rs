use std::path::PathBuf;

fn main() {
    let payload = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("payload").join("app.zip");
    if !payload.exists() {
        std::fs::create_dir_all(payload.parent().expect("payload dir")).expect("create payload dir");
        let file = std::fs::File::create(&payload).expect("create placeholder zip");
        let mut w = zip::ZipWriter::new(file);
        w.finish().expect("finish placeholder zip");
    }
    println!("cargo:rerun-if-changed=payload/app.zip");
    tauri_build::build()
}
