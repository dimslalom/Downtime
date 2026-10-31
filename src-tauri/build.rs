fn main() {
    // Refresh embedded Windows resources when the app artwork changes.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
