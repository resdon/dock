fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // Compile both Slint files.
    slint_build::compile("ui.slint").unwrap();
    slint_build::compile("bluetooth-widget.slint").unwrap();

    // slint_build internally sets SLINT_INCLUDE_GENERATED to the last file compiled.
    // We need to override it back to ui.rs and set a separate var for the bluetooth widget.
    let ui_path = format!("{}/ui.rs", out_dir);
    let bt_path = format!("{}/bluetooth-widget.rs", out_dir);
    println!("cargo:rustc-env=SLINT_INCLUDE_GENERATED={}", ui_path);
    println!("cargo:rustc-env=SLINT_INCLUDE_BLUETOOTH={}", bt_path);
}
