//! Reuse a completed private installation without downloading or running exporters again.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path},
    time::UNIX_EPOCH,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    name: String,
    size: u64,
    modified: u64,
}
#[derive(Serialize, Deserialize)]
struct Complete {
    key: String,
    files: Vec<Entry>,
}
fn scan(root: &Path) -> Option<Vec<Entry>> {
    let meta = fs::symlink_metadata(root).ok()?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return None;
    }
    let mut pending = vec![root.to_path_buf()];
    let mut entries = Vec::new();
    while let Some(dir) = pending.pop() {
        for file in fs::read_dir(dir).ok()? {
            let file = file.ok()?;
            let path = file.path();
            let meta = fs::symlink_metadata(&path).ok()?;
            if meta.file_type().is_symlink() {
                return None;
            }
            if meta.is_dir() {
                pending.push(path);
                continue;
            }
            if !meta.is_file() || entries.len() >= 100_000 {
                return None;
            }
            let relative = path.strip_prefix(root).ok()?;
            if relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            {
                return None;
            }
            entries.push(Entry {
                name: relative.to_str()?.to_owned(),
                size: meta.len(),
                modified: u64::try_from(
                    meta.modified()
                        .ok()?
                        .duration_since(UNIX_EPOCH)
                        .ok()?
                        .as_nanos(),
                )
                .ok()?,
            });
        }
    }
    entries.sort();
    Some(entries)
}
/// The marker is created only after verified official inputs and every exporter/sound step succeed.
pub fn valid(root: &Path, marker: &Path, key: &str) -> bool {
    let Ok(meta) = fs::symlink_metadata(marker) else {
        return false;
    };
    if !meta.is_file() || meta.len() > 32 * 1024 * 1024 {
        return false;
    }
    let Some(saved) = fs::read(marker)
        .ok()
        .and_then(|v| serde_json::from_slice::<Complete>(&v).ok())
    else {
        return false;
    };
    saved.key == key
        && !saved.files.is_empty()
        && scan(root).is_some_and(|files| files == saved.files)
}
pub fn save(root: &Path, marker: &Path, key: &str) -> Result<(), String> {
    let files = scan(root)
        .filter(|f| !f.is_empty())
        .ok_or("Could not inventory prepared installation")?;
    let bytes = serde_json::to_vec(&Complete {
        key: key.to_owned(),
        files,
    })
    .map_err(|e| e.to_string())?;
    let temporary = marker.with_extension("complete-new");
    let _ = fs::remove_file(&temporary);
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(temporary, marker).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_complete_unchanged_installations_are_reused() {
        let root =
            std::env::temp_dir().join(format!("horizon-prepared-cache-{}", std::process::id()));
        let data = root.join("prepared");
        fs::create_dir_all(&data).unwrap();
        let marker = root.join("complete.json");
        fs::write(data.join("facts.json"), b"verified").unwrap();
        assert!(!valid(&data, &marker, "v1"));
        save(&data, &marker, "v1").unwrap();
        assert!(valid(&data, &marker, "v1"));
        assert!(!valid(&data, &marker, "v2"));
        fs::write(data.join("facts.json"), b"changed length").unwrap();
        assert!(!valid(&data, &marker, "v1"));
        save(&data, &marker, "v1").unwrap();
        fs::remove_file(data.join("facts.json")).unwrap();
        assert!(!valid(&data, &marker, "v1"));
        #[cfg(unix)]
        {
            fs::write(root.join("outside"), b"private").unwrap();
            std::os::unix::fs::symlink(root.join("outside"), data.join("facts.json")).unwrap();
            assert!(save(&data, &marker, "v1").is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
