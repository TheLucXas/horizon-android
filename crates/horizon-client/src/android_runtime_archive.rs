//! Bounded extraction of the checksum-pinned Android OpenJDK tar archive.
//! Links become ordinary copies inside a fresh unpublished directory; no archive-created
//! symlink, executable mode, device node or path outside that directory is installed.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

type Check<'a> = &'a dyn Fn() -> Result<(), String>;
#[derive(Clone, Copy)]
struct Limits {
    file: u64,
    total: u64,
    entries: usize,
}
const LIMITS: Limits = Limits {
    file: 256 * 1024 * 1024,
    total: 512 * 1024 * 1024,
    entries: 16384,
};
struct Budget<'a> {
    check: Check<'a>,
    limits: Limits,
    total: u64,
    entries: usize,
}
impl Budget<'_> {
    fn entry(&mut self) -> Result<(), String> {
        (self.check)()?;
        self.entries += 1;
        if self.entries > self.limits.entries {
            return Err("Demasiadas entradas en OpenJDK.".into());
        }
        Ok(())
    }
    fn bytes(&mut self, bytes: u64) -> Result<(), String> {
        self.total = self
            .total
            .checked_add(bytes)
            .ok_or("OpenJDK demasiado grande")?;
        if self.total > self.limits.total {
            return Err("OpenJDK descomprimido demasiado grande.".into());
        }
        Ok(())
    }
}
fn error(error: std::io::Error) -> String {
    error.to_string()
}

/// Normalize a relative archive path, optionally resolving a link's `..` components.
/// Root escapes, absolute paths and excessively deep paths are always rejected.
fn relative(path: &Path, base: &Path, link: bool) -> Result<PathBuf, String> {
    let mut out = base.to_path_buf();
    for part in path.components() {
        match part {
            Component::Normal(name) => {
                if name.to_string_lossy().contains(['\\', ':', '\0']) {
                    return Err("Ruta inválida en OpenJDK.".into());
                }
                out.push(name);
            }
            Component::CurDir => (),
            Component::ParentDir if link && out.pop() => (),
            _ => return Err("Ruta fuera de OpenJDK.".into()),
        }
        if out.components().count() > 64 {
            return Err("Ruta de OpenJDK demasiado profunda.".into());
        }
    }
    Ok(out)
}

fn copy_file(
    reader: &mut impl Read,
    target: &Path,
    expected: u64,
    budget: &mut Budget<'_>,
) -> Result<(), String> {
    if expected > budget.limits.file {
        return Err("Archivo de OpenJDK demasiado grande.".into());
    }
    fs::create_dir_all(target.parent().ok_or("Ruta inválida")?).map_err(error)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(error)?;
    let mut count = 0u64;
    let mut buffer = [0; 65536];
    loop {
        (budget.check)()?;
        let n = reader.read(&mut buffer).map_err(error)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > expected {
            return Err("Tamaño de archivo de OpenJDK inválido.".into());
        }
        budget.bytes(n as u64)?;
        file.write_all(&buffer[..n]).map_err(error)?;
    }
    if count != expected {
        return Err("Archivo de OpenJDK truncado.".into());
    }
    Ok(())
}

fn copy_tree(source: &Path, target: &Path, budget: &mut Budget<'_>) -> Result<(), String> {
    budget.entry()?;
    let meta = fs::symlink_metadata(source).map_err(error)?;
    if meta.is_dir() {
        fs::create_dir(target).map_err(error)?;
        for child in fs::read_dir(source).map_err(error)? {
            let child = child.map_err(error)?;
            copy_tree(&child.path(), &target.join(child.file_name()), budget)?;
        }
    } else if meta.is_file() {
        copy_file(
            &mut File::open(source).map_err(error)?,
            target,
            meta.len(),
            budget,
        )?;
    } else {
        return Err("Enlace o archivo especial en OpenJDK.".into());
    }
    Ok(())
}

/// The destination must not exist. Checks run between entries and each 64 KiB copied, including
/// copies made for archive links. On failure only the newly-created destination is removed.
pub fn extract_tar(reader: impl Read, destination: &Path, check: Check<'_>) -> Result<(), String> {
    extract_with_limits(reader, destination, check, LIMITS)
}

fn extract_with_limits(
    reader: impl Read,
    destination: &Path,
    check: Check<'_>,
    limits: Limits,
) -> Result<(), String> {
    check()?;
    // Never traverse an existing symlink in an installation parent.
    let mut ancestor = PathBuf::new();
    for part in destination.components() {
        ancestor.push(part.as_os_str());
        match fs::symlink_metadata(&ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("Enlace en el destino de OpenJDK.".into());
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(error(e)),
        }
    }
    fs::create_dir(destination).map_err(error)?;
    let result = (|| {
        let mut budget = Budget {
            check,
            limits,
            total: 0,
            entries: 0,
        };
        let mut links = Vec::new();
        let mut archive = tar::Archive::new(reader);
        for entry in archive.entries().map_err(error)? {
            budget.entry()?;
            let mut entry = entry.map_err(error)?;
            let kind = entry.header().entry_type();
            let path = relative(&entry.path().map_err(error)?, Path::new(""), false)?;
            if path.as_os_str().is_empty() {
                if kind.is_dir() {
                    continue;
                }
                return Err("Nombre vacío en OpenJDK.".into());
            }
            let target = destination.join(&path);
            if kind.is_dir() {
                fs::create_dir_all(target).map_err(error)?;
            } else if kind.is_file() {
                let size = entry.size();
                copy_file(&mut entry, &target, size, &mut budget)?;
            } else if kind.is_symlink() || kind.is_hard_link() {
                let link = entry
                    .link_name()
                    .map_err(error)?
                    .ok_or("Enlace sin destino en OpenJDK")?;
                let base = if kind.is_hard_link() {
                    Path::new("")
                } else {
                    path.parent().unwrap_or(Path::new(""))
                };
                let source = relative(&link, base, true)?;
                // Copying a directory into itself/its descendant would recurse forever. The
                // inverse overlap could overwrite a source; no overlapping paths are allowed.
                if path.starts_with(&source) || source.starts_with(&path) {
                    return Err("Enlace recursivo en OpenJDK.".into());
                }
                links.push((path, source));
            } else {
                return Err("Tipo de archivo inesperado en OpenJDK.".into());
            }
        }
        // A link may reference a later archive link. Resolve each dependency at most once per
        // pass; cycles and dangling references end when an entire pass makes no progress.
        while !links.is_empty() {
            let before = links.len();
            let mut pending = Vec::new();
            let unresolved: Vec<_> = links.iter().map(|(target, _)| target.clone()).collect();
            for (target, source) in links {
                check()?;
                let source_path = destination.join(&source);
                if !source_path.exists()
                    || (source_path.is_dir()
                        && unresolved.iter().any(|path| path.starts_with(&source)))
                {
                    pending.push((target, source));
                    continue;
                }
                let target_path = destination.join(&target);
                fs::create_dir_all(target_path.parent().ok_or("Ruta inválida")?).map_err(error)?;
                copy_tree(&source_path, &target_path, &mut budget)?;
            }
            if pending.len() == before {
                return Err("Enlace circular o ausente en OpenJDK.".into());
            }
            links = pending;
        }
        check()
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        io::Cursor,
        sync::atomic::{AtomicUsize, Ordering},
    };
    fn root() -> PathBuf {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "horizon-runtime-test-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        root
    }
    fn tar(entries: &[(&str, u8, &str, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for (name, kind, link, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::new(*kind));
            header.set_size(bytes.len() as u64);
            // Raw header permits malicious paths which Builder::append_path rightly refuses.
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.as_mut_bytes()[157..157 + link.len()].copy_from_slice(link.as_bytes());
            header.set_cksum();
            tar.append(&header, *bytes).unwrap();
        }
        tar.into_inner().unwrap()
    }
    #[test]
    fn copies_files_and_forward_legal_links_without_publishing_symlinks() {
        let root = root();
        let out = root.join("installing");
        let archive = tar(&[
            ("./", b'5', "", b""),
            ("legal/module/LICENSE", b'2', "../base/LICENSE", b""),
            ("legal/other/LICENSE", b'1', "legal/module/LICENSE", b""),
            (
                "legal/base/LICENSE",
                b'0',
                "",
                b"GPL with Classpath Exception",
            ),
        ]);
        extract_tar(Cursor::new(archive), &out, &|| Ok(())).unwrap();
        for file in [
            "legal/base/LICENSE",
            "legal/module/LICENSE",
            "legal/other/LICENSE",
        ] {
            assert_eq!(
                fs::read(out.join(file)).unwrap(),
                b"GPL with Classpath Exception"
            );
            assert!(
                !fs::symlink_metadata(out.join(file))
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_path_escapes_special_entries_and_recursive_links() {
        let root = root();
        let out = root.join("installing");
        let fixtures = [
            tar(&[("../escape", b'0', "", b"bad")]),
            tar(&[("/escape", b'0', "", b"bad")]),
            tar(&[("a", b'2', "../escape", b"")]),
            tar(&[("a", b'1', "/etc/passwd", b"")]),
            tar(&[("a", b'3', "", b"")]),
            tar(&[("a/b", b'2', "..", b"")]),
            tar(&[("a", b'2', "a", b"")]),
            tar(&[("a", b'2', "b", b""), ("b", b'2', "a", b"")]),
        ];
        for fixture in fixtures {
            assert!(extract_tar(Cursor::new(fixture), &out, &|| Ok(())).is_err());
            assert!(!out.exists());
            assert!(!root.join("escape").exists());
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancellation_and_limits_never_leave_an_installation() {
        let root = root();
        let out = root.join("installing");
        let data = vec![7; 140000];
        let archive = tar(&[("modules", b'0', "", &data)]);
        assert!(extract_tar(Cursor::new(&archive), &out, &|| Err("cancelled".into())).is_err());
        assert!(!out.exists());
        let calls = Cell::new(0);
        assert!(
            extract_tar(Cursor::new(&archive), &out, &|| {
                calls.set(calls.get() + 1);
                if calls.get() > 4 {
                    Err("expired".into())
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert!(!out.exists());
        for limits in [
            Limits { file: 5, ..LIMITS },
            Limits { total: 5, ..LIMITS },
            Limits {
                entries: 0,
                ..LIMITS
            },
        ] {
            assert!(extract_with_limits(Cursor::new(&archive), &out, &|| Ok(()), limits).is_err());
            assert!(!out.exists());
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn truncated_input_and_existing_destinations_are_rejected_without_data_loss() {
        let root = root();
        let out = root.join("installing");
        let data = vec![7; 2000];
        let archive = tar(&[("modules", b'0', "", &data)]);
        assert!(extract_tar(Cursor::new(&archive[..600]), &out, &|| Ok(())).is_err());
        assert!(!out.exists());
        fs::create_dir(&out).unwrap();
        fs::write(out.join("keep"), b"existing").unwrap();
        assert!(extract_tar(Cursor::new(&archive), &out, &|| Ok(())).is_err());
        assert_eq!(fs::read(out.join("keep")).unwrap(), b"existing");
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_destination_ancestors() {
        let root = root();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("alias")).unwrap();
        assert!(
            extract_tar(
                Cursor::new(tar(&[])),
                &root.join("alias/installing"),
                &|| Ok(())
            )
            .is_err()
        );
        assert!(!outside.join("installing").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
