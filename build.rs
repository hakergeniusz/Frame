fn main() {
    // libmpv is discovered via pkg-config ('mpv' cbindgen module); this also
    // emits the correct link flags on every distribution.
    let lib = pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("mpv")
        .expect("libmpv >= 2.0 not found via pkg-config ('mpv') — install the mpv/libmpv development package");
    println!("cargo:metadata-libmpv-version={}", lib.version);

    // libglvnd's vendor-neutral GL loaders resolve GL symbols for the render
    // path (both are part of the GTK 4 GL stack on Linux).
    pkg_config::Config::new()
        .atleast_version("1.5")
        .probe("egl")
        .expect("libEGL >= 1.5 not found via pkg-config ('egl') — install libegl/libglvnd development package");
    pkg_config::Config::new()
        .atleast_version("1.0")
        .probe("glx")
        .expect("libGLX not found via pkg-config ('glx') — install libglx/libglvnd development package");

    // Compile the icon GResource bundle at build time.
    let out = std::env::var("OUT_DIR").unwrap();
    let target = format!("{out}/resources.gresource");
    let status = std::process::Command::new("glib-compile-resources")
        .args([
            "--sourcedir",
            "assets",
            "--target",
            &target,
            "--generate",
            "assets/resources.gresource.xml",
        ])
        .status()
        .expect("glib-compile-resources not found — install glib2 development tools");
    assert!(status.success(), "glib-compile-resources failed");
    println!("cargo:rerun-if-changed=assets/resources.gresource.xml");
    println!("cargo:rerun-if-changed=assets/icons");
}
