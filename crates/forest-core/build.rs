// Hashes every core source and asset so the on-disk sprite cache is dropped whenever any of them change.
use std::hash::Hasher;
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn main() {
    let mut files = Vec::new();
    walk(Path::new("src"), &mut files);
    walk(Path::new("assets"), &mut files);
    files.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in &files {
        h.write(f.to_string_lossy().as_bytes());
        h.write(&std::fs::read(f).unwrap_or_default());
    }
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rustc-env=SPRITE_GEN_HASH={:016x}", h.finish());
}
