//! Filesystem helpers: asset discovery and output-path construction.

use crate::diag::WeftError;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A discovered asset file, paired with its path relative to the `assets/` dir.
#[derive(Debug, Clone)]
pub struct Asset {
    /// Path relative to the `assets/` directory (e.g. `reference.md`).
    pub relative: PathBuf,
    /// Absolute source path.
    pub absolute: PathBuf,
}

/// The project's `assets/` directory, if it exists.
fn assets_dir(root: &Path) -> Option<PathBuf> {
    let dir = root.join("assets");
    dir.is_dir().then_some(dir)
}

/// List every file under the project's `assets/` directory (recursively).
pub fn list_assets(root: &Path) -> Result<Vec<Asset>, WeftError> {
    let Some(dir) = assets_dir(root) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for entry in WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file() {
            let absolute = entry.path().to_path_buf();
            let relative = absolute
                .strip_prefix(&dir)
                .unwrap_or(&absolute)
                .to_path_buf();
            out.push(Asset { relative, absolute });
        }
    }
    out.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(out)
}

/// Read an asset's bytes.
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, WeftError> {
    std::fs::read(path).map_err(|source| WeftError::Read {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_assets_dir_yields_empty() {
        let tmp = std::env::temp_dir().join(format!("weaft-fs-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(list_assets(&tmp).unwrap().is_empty());
        drop(std::fs::remove_dir_all(&tmp));
    }
}
