use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let readme_path = Path::new(&manifest_dir).join("../../README.md");
    let output_path = Path::new(&manifest_dir).join("README-crates.md");

    println!("cargo::rerun-if-changed={}", readme_path.display());

    let readme = fs::read_to_string(&readme_path).expect("Failed to read README.md");

    // Strip the logo image tag at the start
    let stripped = readme
        .lines()
        .filter(|line| !line.contains("<img src=\"./assets/logo.png\""))
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(&output_path, stripped).expect("Failed to write README-crates.md");
}
