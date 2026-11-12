// FIX (v1.5.5): Slint has been removed. All rendering is now via wgpu GPU.
// This build script is kept as a no-op for Cargo compatibility.
fn main() {
    // Re-run if this file changes
    println!("cargo:rerun-if-changed=build.rs");
}
