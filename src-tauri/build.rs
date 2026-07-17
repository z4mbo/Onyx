fn main() {
    // Re-embed the production frontend whenever Vite changes the dist tree.
    println!("cargo:rerun-if-changed=../dist");
    tauri_build::build()
}
