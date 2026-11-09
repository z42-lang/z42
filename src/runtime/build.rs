use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn sources(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, files)?;
        } else if path.extension().is_some_and(|x| x == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn field(hash: &mut blake3::Hasher, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest.as_path();
    let mut files = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        root.join("src/host/Cargo.toml"),
        manifest.join("build.rs"),
    ];
    sources(&root.join("src"), &mut files)?;
    println!("cargo:rerun-if-changed={}", root.join("src").display());
    println!("cargo:rerun-if-changed={}", root.join("crates").display());
    for entry in fs::read_dir(root.join("crates"))? {
        let path = entry?.path();
        if path.join("src").is_dir() {
            sources(&path.join("src"), &mut files)?;
        }
        if path.join("Cargo.toml").is_file() {
            files.push(path.join("Cargo.toml"));
        }
    }
    files.sort();
    files.dedup();
    let mut hash = blake3::Hasher::new();
    for file in files {
        println!("cargo:rerun-if-changed={}", file.display());
        field(
            &mut hash,
            file.strip_prefix(root)?.to_string_lossy().as_bytes(),
        );
        field(&mut hash, &fs::read(file)?);
    }
    let rustc = Command::new(env::var("RUSTC")?).arg("-vV").output()?;
    if !rustc.status.success() {
        return Err("cannot identify rustc".into());
    }
    field(&mut hash, &rustc.stdout);
    let target = env::var("TARGET")?;
    field(&mut hash, target.as_bytes());
    field(&mut hash, env::var("CARGO_CFG_TARGET_FEATURE")?.as_bytes());
    let bytes = hash.finalize();
    let generated = format!("pub const COMPONENT_BUILD_ID: [u8; 32] = {:?};\npub const COMPONENT_TARGET: &str = {:?};\n", bytes.as_bytes(), target);
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("identity.rs"),
        generated,
    )?;
    Ok(())
}
