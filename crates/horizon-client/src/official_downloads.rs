//! Verified official inputs for Android's resource preparer. This is not a playable resource
//! installation: the caller must run the exporters and atomically publish their output afterwards.
//! Call only after the account manager has verified Java ownership. No account token is sent here.

use serde_json::Value;
use sha1::{Digest, Sha1};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const VERSION: &str = "26.3";
const CLIENT: &str = "e877b6a07acd633fb3bb475002175cec036e7b87";
const SERVER: &str = "33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c";
const MANIFESTS: &[&str] = &[
    "11f1aefe473eefba0af62e3286c33b692807b2eb",
    "4fe1aa1ef8da1cb95c5bad1fb98890ca56dd8ca3",
    "702fe59163c6ee6578607daa85811d9bc9c7cc40",
    "61fff1316bd50312624a858bbb5eb18fa45208ed",
];
const JSON_LIMIT: u64 = 16 * 1024 * 1024;
const FILE_LIMIT: u64 = 512 * 1024 * 1024;
const TOTAL_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

/// Progress for the current stage, in bytes (or extraction entries).
#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub stage: &'static str,
    pub completed: u64,
    pub total: u64,
}

/// Verified official launcher-style input tree; never treat this as prepared game data.
#[derive(Debug)]
pub struct Downloaded {
    pub minecraft_dir: PathBuf,
    pub client_jar: PathBuf,
    pub server_jar: PathBuf,
    pub assets_dir: PathBuf,
    pub version_manifest: PathBuf,
}

fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Descarga cancelada".into())
    } else {
        Ok(())
    }
}

fn official(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    matches!(
        host,
        "piston-meta.mojang.com"
            | "piston-data.mojang.com"
            | "launcher.mojang.com"
            | "launchermeta.mojang.com"
            | "libraries.minecraft.net"
            | "resources.download.minecraft.net"
            | "resources.download.mojang.com"
    ) && !path.contains(['\\', '?', '#', '%'])
        && !path.split('/').any(|part| part == ".." || part == ".")
}

fn relative(name: &str) -> Result<&Path, String> {
    let path = Path::new(name);
    if name.is_empty()
        || name.contains(['\\', ':', '\0'])
        || path
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("Ruta insegura en los recursos oficiales".into());
    }
    Ok(path)
}

/// Existing symlinks are rejected, including the root; the app owns this private directory.
fn path_in(root: &Path, name: &str) -> Result<PathBuf, String> {
    let path = relative(name)?;
    let mut current = PathBuf::new();
    for part in root.components().chain(path.components()) {
        current.push(part.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("Enlace simbólico en la caché".into());
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("No se pudo inspeccionar la caché".into()),
        }
    }
    Ok(root.join(path))
}

fn hash_valid(hash: &str) -> bool {
    hash.len() == 40
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn verified(path: &Path, hash: &str, size: u64, cancel: &AtomicBool) -> Result<bool, String> {
    let Ok(mut file) = File::open(path) else {
        return Ok(false);
    };
    if file
        .metadata()
        .map_err(|_| "No se pudo leer la caché")?
        .len()
        != size
    {
        return Ok(false);
    }
    let mut digest = Sha1::new();
    let mut buf = [0; 64 * 1024];
    loop {
        cancelled(cancel)?;
        let n = file
            .read(&mut buf)
            .map_err(|_| "No se pudo verificar la caché")?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    Ok(format!("{:x}", digest.finalize()) == hash)
}

trait Network {
    fn open(&self, url: &str) -> Result<Box<dyn Read>, String>;
}
struct OfficialNetwork(ureq::Agent);
impl Network for OfficialNetwork {
    fn open(&self, url: &str) -> Result<Box<dyn Read>, String> {
        if !official(url) {
            return Err("Servidor de descarga no autorizado".into());
        }
        // Redirects are disabled: a trusted URL must not redirect into an untrusted host.
        self.0
            .get(url)
            .call()
            .map(|r| r.into_reader() as Box<dyn Read>)
            .map_err(|_| "No se pudo descargar desde Mojang; vuelve a intentarlo".into())
    }
}

struct Transfer<'a, N> {
    network: N,
    root: &'a Path,
    cancel: &'a AtomicBool,
    progress: &'a dyn Fn(Progress),
    completed: u64,
    total: u64,
}
impl<N: Network> Transfer<'_, N> {
    fn fetch(&mut self, spec: &Value, name: &str, stage: &'static str) -> Result<PathBuf, String> {
        let url = text(spec, "url")?;
        let hash = text(spec, "sha1")?;
        let size = spec["size"].as_u64().ok_or("Falta el tamaño del archivo")?;
        if !official(url) || !hash_valid(hash) || size > FILE_LIMIT {
            return Err("Metadatos de descarga no válidos".into());
        }
        self.total = self
            .total
            .checked_add(size)
            .filter(|&n| n <= TOTAL_LIMIT)
            .ok_or("La descarga excede el límite permitido")?;
        let path = path_in(self.root, name)?;
        if verified(&path, hash, size, self.cancel)? {
            self.completed += size;
            (self.progress)(Progress {
                stage,
                completed: self.completed,
                total: self.total,
            });
            return Ok(path);
        }
        fs::create_dir_all(path.parent().ok_or("Ruta sin directorio")?)
            .map_err(|_| "No se pudo crear la caché")?;
        let part_name = format!("{name}.part");
        let part = path_in(self.root, &part_name)?;
        for attempt in 0..3 {
            cancelled(self.cancel)?;
            let result = (|| {
                let mut reader = self.network.open(url)?;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&part)
                    .map_err(|_| "No se pudo guardar la descarga")?;
                let mut digest = Sha1::new();
                let mut count = 0u64;
                let mut buf = [0; 64 * 1024];
                loop {
                    cancelled(self.cancel)?;
                    let n = reader
                        .read(&mut buf)
                        .map_err(|_| "Se interrumpió la descarga")?;
                    if n == 0 {
                        break;
                    }
                    count += n as u64;
                    if count > size {
                        return Err("El archivo supera su tamaño declarado".into());
                    }
                    file.write_all(&buf[..n])
                        .map_err(|_| "No se pudo guardar la descarga")?;
                    digest.update(&buf[..n]);
                    (self.progress)(Progress {
                        stage,
                        completed: self.completed + count,
                        total: self.total,
                    });
                }
                if count != size || format!("{:x}", digest.finalize()) != hash {
                    return Err("La verificación SHA-1 de Mojang falló".into());
                }
                file.sync_all()
                    .map_err(|_| "No se pudo completar la descarga")?;
                fs::rename(&part, &path)
                    .map_err(|_| "No se pudo publicar el archivo verificado")?;
                Ok(())
            })();
            match result {
                Ok(()) => {
                    self.completed += size;
                    return Ok(path);
                }
                Err(e) => {
                    let _ = fs::remove_file(&part);
                    cancelled(self.cancel)?;
                    if attempt == 2 {
                        return Err(e);
                    }
                    pause(self.cancel, Duration::from_millis(250 * (1 << attempt)))?;
                }
            }
        }
        unreachable!()
    }
}
fn pause(cancel: &AtomicBool, duration: Duration) -> Result<(), String> {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        cancelled(cancel)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("Falta el campo oficial {key}"))
}
fn json_file(path: &Path) -> Result<Value, String> {
    let file = File::open(path).map_err(|_| "No se pudo abrir el manifiesto")?;
    if file
        .metadata()
        .map_err(|_| "No se pudo leer el manifiesto")?
        .len()
        > JSON_LIMIT
    {
        return Err("Manifiesto demasiado grande".into());
    }
    serde_json::from_reader(file).map_err(|_| "Manifiesto JSON no válido".into())
}

fn manifest_bytes(
    network: &impl Network,
    url: &str,
    hash: &str,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    for attempt in 0..3 {
        cancelled(cancel)?;
        let result = (|| {
            let mut bytes = Vec::new();
            let mut reader = network.open(url)?;
            let mut buf = [0; 64 * 1024];
            loop {
                cancelled(cancel)?;
                let n = reader
                    .read(&mut buf)
                    .map_err(|_| "Se interrumpió el manifiesto")?;
                if n == 0 {
                    break;
                }
                if bytes.len() as u64 + n as u64 > JSON_LIMIT {
                    return Err("Manifiesto demasiado grande".into());
                }
                bytes.extend_from_slice(&buf[..n]);
            }
            if format!("{:x}", Sha1::digest(&bytes)) != hash {
                return Err("El manifiesto no coincide con la versión fijada".into());
            }
            Ok(bytes)
        })();
        match result {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                cancelled(cancel)?;
                if attempt == 2 {
                    return Err(e);
                }
                pause(cancel, Duration::from_millis(250 * (1 << attempt)))?;
            }
        }
    }
    unreachable!()
}

/// Downloads only the version pinned by research-lock.json. Complete verified files are reused
/// after interruption; partial files are restarted. The owner must serialize setup jobs per root.
/// Downloading is deliberately separate from the ownership-gated setup coordinator.
pub fn download(
    cache: &Path,
    cancel: &AtomicBool,
    progress: impl Fn(Progress),
) -> Result<Downloaded, String> {
    let network = OfficialNetwork(
        ureq::AgentBuilder::new()
            .redirects(0)
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(30))
            .build(),
    );
    let mut transfer = Transfer {
        network,
        root: cache,
        cancel,
        progress: &progress,
        completed: 0,
        total: 0,
    };
    cancelled(cancel)?;
    // This immutable, pinned manifest is also listed by Mojang's public version manifest.
    // Fetch it directly so a later republishing cannot silently change this game's specification.
    let hash = MANIFESTS[0];
    let url = format!("https://piston-meta.mojang.com/v1/packages/{hash}/{VERSION}.json");
    let bytes = manifest_bytes(&transfer.network, &url, hash, cancel)?;
    let manifest: Value = serde_json::from_slice(&bytes).map_err(|_| "Manifiesto no válido")?;
    let version_manifest = path_in(cache, "versions/26.3/26.3.json")?;
    fs::create_dir_all(version_manifest.parent().unwrap())
        .map_err(|_| "No se pudo crear la caché")?;
    let temp = path_in(cache, "versions/26.3/26.3.json.part")?;
    fs::write(&temp, bytes).map_err(|_| "No se pudo guardar el manifiesto")?;
    fs::rename(temp, &version_manifest).map_err(|_| "No se pudo guardar el manifiesto")?;
    if manifest["id"] != VERSION
        || text(&manifest["downloads"]["client"], "sha1")? != CLIENT
        || text(&manifest["downloads"]["server"], "sha1")? != SERVER
    {
        return Err("La versión oficial no coincide con Horizon".into());
    }
    let client_jar = transfer.fetch(
        &manifest["downloads"]["client"],
        "versions/26.3/26.3.jar",
        "Cliente",
    )?;
    let server_jar = transfer.fetch(
        &manifest["downloads"]["server"],
        "versions/26.3/server.jar",
        "Servidor",
    )?;
    let libraries = manifest["libraries"]
        .as_array()
        .ok_or("Faltan las bibliotecas oficiales")?;
    for library in libraries {
        // Headless exporters need Java artifacts, never OS-specific LWJGL native classifiers.
        if library["natives"].is_object()
            || library["rules"]
                .as_array()
                .is_some_and(|rules| rules.iter().any(|r| r["os"].is_object()))
        {
            continue;
        }
        let artifact = &library["downloads"]["artifact"];
        if artifact.is_object() {
            transfer.fetch(
                artifact,
                &format!("libraries/{}", text(artifact, "path")?),
                "Bibliotecas",
            )?;
        }
    }
    let index = &manifest["assetIndex"];
    let id = text(index, "id")?;
    if relative(id)?.components().count() != 1 {
        return Err("Identificador de recursos no válido".into());
    }
    let index_path = transfer.fetch(
        index,
        &format!("assets/indexes/{id}.json"),
        "Índice de recursos",
    )?;
    let objects = json_file(&index_path)?;
    let objects = objects["objects"]
        .as_object()
        .ok_or("Índice de recursos no válido")?;
    for (name, object) in objects {
        relative(name)?;
        let hash = text(object, "hash")?;
        if !hash_valid(hash) {
            return Err("SHA-1 de recurso no válido".into());
        }
        let spec = serde_json::json!({"url":format!("https://resources.download.minecraft.net/{}/{hash}", &hash[..2]), "sha1":hash,"size":object["size"]});
        transfer.fetch(
            &spec,
            &format!("assets/objects/{}/{hash}", &hash[..2]),
            "Recursos",
        )?;
    }
    Ok(Downloaded {
        minecraft_dir: cache.to_owned(),
        client_jar,
        server_jar,
        assets_dir: cache.join("assets"),
        version_manifest,
    })
}

/// Extracts ONLY public-resource paths from the verified client jar, never executable classes.
/// The destination must be an unpublished staging directory, not an existing player's resources.
pub fn extract_client_assets(
    inputs: &Downloaded,
    staging: &Path,
    cancel: &AtomicBool,
    progress: impl Fn(Progress),
) -> Result<(), String> {
    let manifest = json_file(&inputs.version_manifest)?;
    let client_size = manifest["downloads"]["client"]["size"]
        .as_u64()
        .ok_or("Tamaño del cliente no válido")?;
    if !verified(&inputs.client_jar, CLIENT, client_size, cancel)? {
        return Err("El cliente oficial se ha dañado; descarga de nuevo".into());
    }
    extract_jar_assets(&inputs.client_jar, staging, cancel, &progress)?;
    // Asset-index objects override the jar's placeholders (notably Unifont and languages).
    let index = json_file(
        &inputs
            .assets_dir
            .join("indexes")
            .join(format!("{}.json", text(&manifest["assetIndex"], "id")?)),
    )?;
    for (name, object) in index["objects"].as_object().ok_or("Índice no válido")? {
        cancelled(cancel)?;
        let hash = text(object, "hash")?;
        if !hash_valid(hash) {
            return Err("Hash de recurso no válido".into());
        }
        let source = path_in(
            &inputs.assets_dir,
            &format!("objects/{}/{hash}", &hash[..2]),
        )?;
        let size = object["size"]
            .as_u64()
            .filter(|&n| n <= FILE_LIMIT)
            .ok_or("Tamaño de recurso no válido")?;
        if !verified(&source, hash, size, cancel)? {
            return Err("Recurso de caché dañado".into());
        }
        let path = path_in(staging, &format!("assets/{name}"))?;
        fs::create_dir_all(path.parent().unwrap()).map_err(|_| "No se pudo crear el recurso")?;
        fs::copy(source, path).map_err(|_| "No se pudo instalar el recurso")?;
    }
    Ok(())
}

fn extract_jar_assets(
    jar: &Path,
    staging: &Path,
    cancel: &AtomicBool,
    progress: &dyn Fn(Progress),
) -> Result<(), String> {
    let file = File::open(jar).map_err(|_| "Falta el cliente verificado")?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "Cliente ZIP no válido")?;
    let mut total = 0u64;
    let entries = zip.len();
    for i in 0..entries {
        cancelled(cancel)?;
        let mut entry = zip.by_index(i).map_err(|_| "Entrada ZIP no válida")?;
        let name = entry.name().to_owned();
        if !name.starts_with("assets/") || entry.is_dir() {
            continue;
        }
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("Enlace simbólico en los recursos".into());
        }
        let path = path_in(staging, &name)?;
        total = total
            .checked_add(entry.size())
            .filter(|&n| n <= TOTAL_LIMIT)
            .ok_or("Los recursos descomprimidos exceden el límite")?;
        if entry.size() > FILE_LIMIT {
            return Err("Recurso demasiado grande".into());
        }
        fs::create_dir_all(path.parent().unwrap()).map_err(|_| "No se pudo crear el recurso")?;
        let mut output = File::create(path).map_err(|_| "No se pudo extraer el recurso")?;
        let mut count = 0;
        let mut buf = [0; 64 * 1024];
        loop {
            cancelled(cancel)?;
            let n = entry.read(&mut buf).map_err(|_| "Recurso ZIP dañado")?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > FILE_LIMIT {
                return Err("Recurso descomprimido demasiado grande".into());
            }
            output
                .write_all(&buf[..n])
                .map_err(|_| "No se pudo guardar el recurso")?;
        }
        progress(Progress {
            stage: "Extracción",
            completed: i as u64 + 1,
            total: entries as u64,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io::Cursor;
    struct Mock {
        bytes: Vec<u8>,
        calls: Cell<usize>,
        fail_first: bool,
    }
    impl Network for Mock {
        fn open(&self, _: &str) -> Result<Box<dyn Read>, String> {
            self.calls.set(self.calls.get() + 1);
            if self.fail_first && self.calls.get() == 1 {
                return Err("transient".into());
            }
            Ok(Box::new(Cursor::new(self.bytes.clone())))
        }
    }
    fn root() -> PathBuf {
        static ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "horizon-download-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn spec(bytes: &[u8]) -> Value {
        serde_json::json!({"url":"https://piston-data.mojang.com/v1/a.jar","sha1":format!("{:x}",Sha1::digest(bytes)),"size":bytes.len()})
    }
    #[test]
    fn extraction_copies_resources_only_and_rejects_zip_traversal() {
        let dir = root();
        let jar = dir.join("client.jar");
        for malicious in [false, true] {
            let mut zip = zip::ZipWriter::new(File::create(&jar).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("net/minecraft/Client.class", opts).unwrap();
            zip.write_all(b"executable").unwrap();
            zip.start_file(
                if malicious {
                    "assets/../../escape"
                } else {
                    "assets/minecraft/lang/en_us.json"
                },
                opts,
            )
            .unwrap();
            zip.write_all(b"resource").unwrap();
            zip.finish().unwrap();
            let staging = dir.join(if malicious { "bad" } else { "good" });
            let result = extract_jar_assets(&jar, &staging, &AtomicBool::new(false), &|_| ());
            if malicious {
                assert!(result.is_err());
                assert!(!dir.join("escape").exists());
            } else {
                result.unwrap();
                assert_eq!(
                    fs::read(staging.join("assets/minecraft/lang/en_us.json")).unwrap(),
                    b"resource"
                );
            }
            assert!(!staging.join("net").exists());
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn constants_match_research_lock() {
        let lock: Value =
            serde_json::from_str(include_str!("../../../research-lock.json")).unwrap();
        assert_eq!(lock["minecraft"]["version"], VERSION);
        assert_eq!(lock["minecraft"]["client_sha1"], CLIENT);
        assert_eq!(lock["minecraft"]["server_sha1"], SERVER);
        assert_eq!(lock["minecraft"]["manifest_sha1"], MANIFESTS[0]);
    }
    #[test]
    fn manifest_transport_retry_keeps_exact_pinned_digest() {
        let network = Mock {
            bytes: b"manifest".to_vec(),
            calls: Cell::new(0),
            fail_first: true,
        };
        let hash = format!("{:x}", Sha1::digest(b"manifest"));
        let bytes = manifest_bytes(
            &network,
            "https://piston-meta.mojang.com/a",
            &hash,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(bytes, b"manifest");
        assert_eq!(network.calls.get(), 2);
    }
    #[test]
    fn rejects_untrusted_hosts_paths_and_urls() {
        for url in [
            "http://piston-data.mojang.com/a",
            "https://piston-data.mojang.com.evil.org/a",
            "https://user@piston-data.mojang.com/a",
            "https://piston-data.mojang.com:443/a",
            "https://piston-data.mojang.com/%2e/a",
            "https://piston-data.mojang.com/../a",
        ] {
            assert!(!official(url), "{url}");
        }
        for path in ["../x", "/a", "C:/a", "a\\b", "a/../../b"] {
            assert!(relative(path).is_err());
        }
    }
    #[test]
    fn retries_then_reuses_verified_cache_and_repairs_corruption() {
        let dir = root();
        let cancel = AtomicBool::new(false);
        let mut t = Transfer {
            network: Mock {
                bytes: b"good".to_vec(),
                calls: Cell::new(0),
                fail_first: true,
            },
            root: &dir,
            cancel: &cancel,
            progress: &|_| (),
            completed: 0,
            total: 0,
        };
        let file = t.fetch(&spec(b"good"), "a/file", "test").unwrap();
        assert_eq!(t.network.calls.get(), 2);
        t.fetch(&spec(b"good"), "a/file", "test").unwrap();
        assert_eq!(t.network.calls.get(), 2);
        fs::write(&file, b"evil").unwrap();
        t.fetch(&spec(b"good"), "a/file", "test").unwrap();
        assert_eq!(fs::read(file).unwrap(), b"good");
        assert_eq!(t.network.calls.get(), 3);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rejects_wrong_digest_and_oversized_body_without_publishing() {
        let dir = root();
        let cancel = AtomicBool::new(false);
        for bytes in [b"evil".to_vec(), b"toolong".to_vec()] {
            let mut t = Transfer {
                network: Mock {
                    bytes,
                    calls: Cell::new(0),
                    fail_first: false,
                },
                root: &dir,
                cancel: &cancel,
                progress: &|_| (),
                completed: 0,
                total: 0,
            };
            assert!(t.fetch(&spec(b"good"), "file", "test").is_err());
            assert!(!dir.join("file").exists());
            assert!(!dir.join("file.part").exists());
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn cancellation_never_opens_network() {
        let dir = root();
        let cancel = AtomicBool::new(true);
        let mut t = Transfer {
            network: Mock {
                bytes: vec![],
                calls: Cell::new(0),
                fail_first: false,
            },
            root: &dir,
            cancel: &cancel,
            progress: &|_| (),
            completed: 0,
            total: 0,
        };
        assert!(t.fetch(&spec(b"good"), "file", "test").is_err());
        assert_eq!(t.network.calls.get(), 0);
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn android_system_alias_resolves_before_checking_private_cache() {
        let dir = root();
        let physical = dir.join("data");
        fs::create_dir_all(physical.join("app/files")).unwrap();
        let alias = dir.join("user-0");
        std::os::unix::fs::symlink(&physical, &alias).unwrap();
        let android_private = alias.join("app/files");
        // NativeActivity supplies a trusted app directory, not an archive or download path.
        let private = fs::canonicalize(android_private).unwrap();
        let cache = private.join("installation/minecraft");
        let cancel = AtomicBool::new(false);
        let mut transfer = Transfer {
            network: Mock {
                bytes: b"good".to_vec(),
                calls: Cell::new(0),
                fail_first: false,
            },
            root: &cache,
            cancel: &cancel,
            progress: &|_| (),
            completed: 0,
            total: 0,
        };
        let file = transfer
            .fetch(&spec(b"good"), "versions/client", "test")
            .unwrap();
        assert_eq!(fs::read(file).unwrap(), b"good");
        let outside = dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, cache.join("escape")).unwrap();
        assert!(
            transfer
                .fetch(&spec(b"good"), "escape/file", "test")
                .is_err()
        );
        assert_eq!(transfer.network.calls.get(), 1);
        assert!(!outside.join("file").exists());
        // A symlink in the cache root itself remains forbidden as well.
        let bad_root = private.join("linked-cache");
        std::os::unix::fs::symlink(&outside, &bad_root).unwrap();
        assert!(path_in(&bad_root, "file").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_destination() {
        let dir = root();
        std::os::unix::fs::symlink(std::env::temp_dir(), dir.join("escape")).unwrap();
        assert!(path_in(&dir, "escape/file").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
