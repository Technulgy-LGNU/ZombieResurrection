use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../apps/zr-review-web/src");
    println!("cargo:rerun-if-changed=../../apps/zr-review-web/package.json");
    println!("cargo:rerun-if-changed=../../apps/zr-review-web/index.html");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let web_dir = manifest_dir.join("../../apps/zr-review-web");

    let install = Command::new("npm")
        .arg("install")
        .current_dir(&web_dir)
        .status()
        .expect("failed to run npm install for embedded frontend");
    assert!(
        install.success(),
        "npm install failed for embedded frontend"
    );

    let build = Command::new("npm")
        .arg("run")
        .arg("build")
        .current_dir(&web_dir)
        .status()
        .expect("failed to run npm run build for embedded frontend");
    assert!(
        build.success(),
        "npm run build failed for embedded frontend"
    );
}
