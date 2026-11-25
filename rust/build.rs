// Link the shared application icon into the Windows executable as well as the runtime window.
use std::{env, ffi::OsString, fs, path::PathBuf, process::Command};

fn resource_compiler() -> OsString {
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return "rc.exe".into();
    }
    let sdk = env::var_os("WindowsSdkDir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("ProgramFiles(x86)").unwrap()).join("Windows Kits/10")
        });
    let mut paths = fs::read_dir(sdk.join("bin"))
        .expect("Windows SDK bin directory")
        .flatten()
        .map(|v| v.path().join("x64/rc.exe"))
        .filter(|v| v.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .pop()
        .expect("Windows SDK resource compiler rc.exe")
        .into_os_string()
}
// Supersample the SVG strokes once during the build. The native UI tints the
// resulting alpha masks for the current button state and monitor scale.
fn compile_icons(out: &std::path::Path) {
    for name in [
        "copied",
        "capture",
        "increase",
        "decrease",
        "sidebar-open",
        "sidebar-close",
        "search",
        "settings",
        "previous",
        "next",
    ] {
        let source = format!("src/native_ui/icons/{name}.svg");
        println!("cargo:rerun-if-changed={source}");
        let bytes = fs::read(&source).expect("read toolbar SVG");
        let tree =
            resvg::usvg::Tree::from_data(&bytes, &Default::default()).expect("parse toolbar SVG");
        let mut image = resvg::tiny_skia::Pixmap::new(96, 96).unwrap();
        let scale = resvg::tiny_skia::Transform::from_scale(
            96. / tree.size().width(),
            96. / tree.size().height(),
        );
        resvg::render(&tree, scale, &mut image.as_mut());
        let mask: Vec<_> = image.pixels().iter().map(|pixel| pixel.alpha()).collect();
        fs::write(out.join(format!("{name}.alpha")), mask).expect("write toolbar icon");
    }
}
fn main() {
    println!("cargo:rerun-if-changed=../resources/app.ico");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    compile_icons(&out);
    let icon = fs::canonicalize("../resources/app.ico").unwrap();
    fs::copy(&icon, out.join("app.ico")).unwrap();
    println!("cargo:rerun-if-changed=src/native_ui/dialogs.rc");
    fs::copy("src/native_ui/dialogs.rc", out.join("app.rc")).unwrap();
    println!("cargo:rerun-if-changed=src/native_ui/app.manifest");
    fs::copy("src/native_ui/app.manifest", out.join("app.manifest")).unwrap();
    let version = env!("CARGO_PKG_VERSION");
    let numeric = format!(
        "{},{},{},0",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH")
    );
    fs::write(
        out.join("version.rcinc"),
        format!("#define CE_NUMVERSION {numeric}\n#define CE_TEXTVERSION \"{version}\"\n"),
    )
    .unwrap();
    let compiler = resource_compiler();
    for (bin, res) in [
        ("clip-everything", "app.res"),
        ("uninstall", "uninstall.res"),
    ] {
        let mut rc = Command::new(&compiler);
        if bin == "uninstall" {
            rc.args(["/d", "CE_UNINSTALLER"]);
        }
        let status = rc
            .current_dir(&out)
            .args(["/nologo", "/fo", res, "app.rc"])
            .status()
            .expect("Windows SDK resource compiler");
        assert!(status.success(), "Resource compilation failed for {bin}");
        println!("cargo:rustc-link-arg-bin={bin}={}", out.join(res).display());
    }
}
