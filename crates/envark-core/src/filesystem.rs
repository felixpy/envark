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
    format!("{kind}-{}", hex::encode(hash.finalize()))
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
    measure_with(path, cancel, |_, _| Ok(()))
}

pub(crate) fn measure_with(
    path: &Path,
    cancel: &CancellationToken,
    mut inspect: impl FnMut(&Path, &std::fs::Metadata) -> Result<()>,
) -> Result<Measurement> {
    reject_links(path)?;
    let mut result = Measurement {
        complete: true,
        ..Measurement::default()
    };
    let mut fingerprint = Sha256::new();
    // Link entries contribute no target data; no symlink or junction is followed.
    let mut iter = WalkDir::new(path)
        .follow_links(false)
        .max_open(16)
        .sort_by_file_name()
        .into_iter();
    while let Some(entry) = iter.next() {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        match entry.and_then(|e| e.metadata().map(|m| (e, m))) {
            Ok((entry, meta)) => {
                inspect(entry.path(), &meta)?;
                let relative = entry.path().strip_prefix(path).unwrap_or(entry.path());
                let name = relative.as_os_str().as_encoded_bytes();
                fingerprint.update(name.len().to_le_bytes());
                fingerprint.update(name);
                if is_link(&meta) {
                    // Fingerprint the link itself, never the data it points to.
                    if meta.is_dir() {
                        iter.skip_current_dir();
                    }
                    fingerprint.update([2]);
                    match std::fs::read_link(entry.path()) {
                        Ok(target) => fingerprint.update(target.as_os_str().as_encoded_bytes()),
                        Err(_) => {
                            result.complete = false;
                            result.skipped += 1;
                        }
                    }
                    continue;
                }
                fingerprint.update([u8::from(meta.is_file())]);
                fingerprint.update(meta.len().to_le_bytes());
                if let Ok(timestamp) = meta
                    .modified()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(std::io::Error::other))
                {
                    fingerprint.update(timestamp.as_nanos().to_le_bytes());
                } else {
                    result.complete = false;
                    result.skipped += 1;
                }
                if meta.is_file() {
                    result.bytes = result.bytes.saturating_add(meta.len());
                    result.files += 1;
                }
            }
            Err(_) => {
                result.complete = false;
                result.skipped += 1;
            }
        }
    }
    result.fingerprint = Some(hex::encode(fingerprint.finalize()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_preserve_the_existing_sha256_encoding() {
        assert_eq!(
            id_for("project", Path::new("workspace")),
            "project-16d26c0c26631901e51ec942c98bfe3c1b53d23cbd1de846b674e6396ef204d8"
        );
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn file_fingerprints_preserve_the_existing_sha256_encoding() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("fixture");
        fs::write(&path, "abc").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)),
            )
            .unwrap();
        let measured =
            measure(&fs::canonicalize(&path).unwrap(), &CancellationToken::new()).unwrap();
        assert!(measured.complete);
        assert_eq!(
            measured.fingerprint.as_deref(),
            Some("7ef158265486268fef7daf55fe089dfab9678e9f2fa34e8de4a3e271634aba52")
        );
    }

    #[test]
    fn equal_size_changes_in_nested_files_change_the_fingerprint() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("package/index.js");
        fs::create_dir(path.parent().unwrap()).unwrap();
        fs::write(&path, "before").unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let before = measure(&root_path, &CancellationToken::new()).unwrap();
        fs::write(&path, "after!").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)),
            )
            .unwrap();
        let after = measure(&root_path, &CancellationToken::new()).unwrap();
        assert_eq!(before.bytes, after.bytes);
        assert_ne!(before.fingerprint, after.fingerprint);
    }

    #[test]
    fn containment_rejects_the_root_and_parent_traversal() {
        let root = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(root.path()).unwrap();
        assert!(contained_directory(&path, &path).is_err());
        let traversal =
            std::path::PathBuf::from(format!("{}/child/../other", root.path().display()));
        assert!(reject_links(&traversal).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_measured_and_cannot_be_cleanup_roots() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("source"), "private source").unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        let link = root_path.join("node_modules");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        assert_eq!(
            measure(&root_path, &CancellationToken::new())
                .unwrap()
                .bytes,
            0
        );
        assert!(contained_directory(&root_path, &link).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn link_targets_are_not_counted_and_later_siblings_are_not_skipped() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let canonical_root = fs::canonicalize(root.path()).unwrap();
        fs::write(outside.path().join("large"), vec![0u8; 4096]).unwrap();
        symlink(outside.path(), root.path().join("a-directory-link")).unwrap();
        symlink(
            outside.path().join("large"),
            root.path().join("b-file-link"),
        )
        .unwrap();
        symlink(
            outside.path().join("missing"),
            root.path().join("c-broken-link"),
        )
        .unwrap();
        fs::create_dir(root.path().join("z-directory")).unwrap();
        fs::write(root.path().join("z-file"), "123").unwrap();
        fs::write(root.path().join("z-directory/file"), "12345").unwrap();
        let measured = measure(&canonical_root, &CancellationToken::new()).unwrap();
        assert!(measured.complete);
        assert_eq!(measured.files, 2);
        assert_eq!(measured.bytes, 8);
        fs::remove_file(root.path().join("b-file-link")).unwrap();
        symlink(
            outside.path().join("other"),
            root.path().join("b-file-link"),
        )
        .unwrap();
        assert_ne!(
            measured.fingerprint,
            measure(&canonical_root, &CancellationToken::new())
                .unwrap()
                .fingerprint
        );
    }
}
