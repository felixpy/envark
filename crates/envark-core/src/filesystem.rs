use crate::{Error, Result, model::Measurement};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path},
    time::UNIX_EPOCH,
};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

pub fn id_for(kind: &str, path: &Path) -> String {
    let mut hash = Sha256::new();
    hash.update(kind.as_bytes());
    hash.update(path.to_string_lossy().as_bytes());
    format!("{kind}-{:x}", hash.finalize())
}

pub fn modified(path: &Path) -> Option<u64> {
    fs::symlink_metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|t| t.as_secs())
}

pub fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Junctions and other reparse points must never become traversal roots.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn reject_links(path: &Path) -> Result<()> {
    if !path.is_absolute() || path.components().any(|p| matches!(p, Component::ParentDir)) {
        return Err(Error::unsafe_path(
            path,
            "an absolute normalized path is required",
        ));
    }
    let mut current = std::path::PathBuf::new();
    for part in path.components() {
        current.push(part.as_os_str());
        if let Ok(meta) = fs::symlink_metadata(&current)
            && is_link(&meta)
        {
            return Err(Error::unsafe_path(
                &current,
                "symbolic links and junctions are not eligible",
            ));
        }
    }
    Ok(())
}

pub fn contained_directory(root: &Path, path: &Path) -> Result<std::path::PathBuf> {
    reject_links(root)?;
    reject_links(path)?;
    let root = fs::canonicalize(root)?;
    let target = fs::canonicalize(path)?;
    if target == root || !target.starts_with(&root) || !target.is_dir() {
        return Err(Error::unsafe_path(
            path,
            "the directory must remain inside its approved root",
        ));
    }
    Ok(target)
}

pub fn measure(path: &Path, cancel: &CancellationToken) -> Result<Measurement> {
    reject_links(path)?;
    let mut result = Measurement {
        complete: true,
        ..Measurement::default()
    };
    // Link entries contribute no target data; no symlink or junction is followed.
    let iter = WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.path()
                .symlink_metadata()
                .map(|m| !is_link(&m))
                .unwrap_or(true)
        });
    for entry in iter {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        match entry.and_then(|e| e.metadata().map(|m| (e, m))) {
            Ok((_, meta)) if meta.is_file() => {
                result.bytes = result.bytes.saturating_add(meta.len());
                result.files += 1;
            }
            Ok(_) => (),
            Err(_) => {
                result.complete = false;
                result.skipped += 1;
            }
        }
    }
    Ok(result)
}

pub fn read_small(path: &Path, max_bytes: u64) -> Result<String> {
    use std::io::Read;
    let file = fs::File::open(path)?;
    let mut content = String::new();
    file.take(max_bytes + 1).read_to_string(&mut content)?;
    if content.len() as u64 > max_bytes {
        return Err(Error::InvalidInput(format!(
            "{} exceeds the size limit",
            path.display()
        )));
    }
    Ok(content)
}
