// MIT/Apache2 License

#[path = "build/pci_ids.rs"]
mod pci_ids;

use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/pci_ids.rs");

    // create the "auto" directory
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let mut auto_path: PathBuf = out_dir.into();
    auto_path.push("auto");
    fs::create_dir_all(&auto_path)?;

    println!(
        "cargo:rustc-env=TARGET={}",
        env::var("TARGET").unwrap()
    );

    pci_ids::process_pci_ids(&auto_path)?;
    Ok(())
}
