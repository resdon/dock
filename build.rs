fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR environment variable not set by Cargo");

    // Compile both Slint files.
    slint_build::compile("ui.slint").expect("failed to compile ui.slint");
    slint_build::compile("bluetooth-widget.slint")
        .expect("failed to compile bluetooth-widget.slint");

    // slint_build internally sets SLINT_INCLUDE_GENERATED to the last file compiled.
    // We need to override it back to ui.rs and set a separate var for the bluetooth widget.
    let ui_path = format!("{}/ui.rs", out_dir);
    let bt_path = format!("{}/bluetooth-widget.rs", out_dir);
    println!("cargo:rustc-env=SLINT_INCLUDE_GENERATED={}", ui_path);
    println!("cargo:rustc-env=SLINT_INCLUDE_BLUETOOTH={}", bt_path);
}
