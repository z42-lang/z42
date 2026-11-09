use std::io::Result;

pub const HAS_FS: bool = true;

pub const PATH_LIST_SEP: char = ';';

pub fn is_name_too_long(_: &std::io::Error) -> bool {
    true
}

pub fn sync_before_rename(file: &std::fs::File) -> Result<()> {
    file.sync_all()
}

pub fn make_executable(path: &str) -> Result<()> {
    let _ = path;
    Ok(())
}

pub fn symlink(src: &str, dst: &str) -> Result<()> {
    let _ = (src, dst);
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "File.SymLink: not implemented on this platform",
    ))
}
