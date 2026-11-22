use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use tw_core::models::{Compression, ModelInfo, ModelKind, ModelStatus};

use crate::{ModelsError, Result};

const PARTIAL_SUFFIX: &str = ".partial";
const IMPORT_SUFFIX: &str = ".importing";

/// Where model files live: `<root>/whisper/*.bin` and `<root>/llm/*.gguf`,
/// with unfinished downloads next to them as `<file>.partial`.
#[derive(Debug, Clone)]
pub struct Inventory {
    root: PathBuf,
}

impl Inventory {
    /// `root` is `<app data>/models`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn dir(&self, kind: ModelKind) -> PathBuf {
        self.root.join(kind.dir_name())
    }

    pub fn path(&self, kind: ModelKind, file: &str) -> Result<PathBuf> {
        check_file_name(file)?;
        Ok(self.dir(kind).join(file))
    }

    /// Partially downloaded data, resumed with an HTTP Range request.
    pub fn partial_path(&self, kind: ModelKind, file: &str) -> Result<PathBuf> {
        check_file_name(file)?;
        Ok(self.dir(kind).join(format!("{file}{PARTIAL_SUFFIX}")))
    }

    /// The catalog with each model's status on disk, followed by the model
    /// files the catalog doesn't know (imported or copied in by hand).
    ///
    /// A finished file whose size differs from the catalog's is reported as
    /// `Failed`; an unfinished download as `Paused`, so the UI can offer
    /// "Resume". Active downloads are the caller's to overlay.
    pub fn models(&self, catalog: &[ModelInfo]) -> Result<Vec<ModelInfo>> {
        let mut models = Vec::with_capacity(catalog.len());
        for entry in catalog {
            let mut model = entry.clone();
            model.status = self.status(entry)?;
            models.push(model);
        }
        for kind in [ModelKind::Voice, ModelKind::Language] {
            let entries = match fs::read_dir(self.dir(kind)) {
                Ok(entries) => entries,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let mut custom = Vec::new();
            for entry in entries {
                let entry = entry?;
                let Ok(file) = entry.file_name().into_string() else {
                    continue;
                };
                let known = catalog.iter().any(|m| m.kind == kind && m.file == file);
                if known || !is_model_file(kind, &file) || !entry.file_type()?.is_file() {
                    continue;
                }
                custom.push(custom_model(kind, &file, entry.metadata()?.len()));
            }
            custom.sort_by(|a, b| a.file.cmp(&b.file));
            models.extend(custom);
        }
        Ok(models)
    }

    fn status(&self, model: &ModelInfo) -> Result<ModelStatus> {
        if let Some(size) = file_size(&self.path(model.kind, &model.file)?)? {
            return Ok(if size == model.size_bytes {
                ModelStatus::Downloaded
            } else {
                ModelStatus::Failed {
                    message: "The file is damaged. Delete it and download it again.".to_owned(),
                }
            });
        }
        Ok(
            match file_size(&self.partial_path(model.kind, &model.file)?)? {
                Some(received) => ModelStatus::Paused {
                    received,
                    total: model.size_bytes,
                },
                None => ModelStatus::NotDownloaded,
            },
        )
    }

    /// Removes the model file and any unfinished download of it.
    pub fn delete(&self, kind: ModelKind, file: &str) -> Result<()> {
        let mut found = false;
        for path in [self.path(kind, file)?, self.partial_path(kind, file)?] {
            match fs::remove_file(path) {
                Ok(()) => found = true,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        if found {
            Ok(())
        } else {
            Err(ModelsError::NotFound {
                file: file.to_owned(),
            })
        }
    }

    /// Copies a user's `.bin` (Whisper) or `.gguf` (language model) file in,
    /// after checking it really is one. Returns it as a custom model.
    pub fn import(&self, source: &Path, kind: ModelKind) -> Result<ModelInfo> {
        let file = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ModelsError::InvalidFileName(source.display().to_string()))?;
        check_file_name(file)?;
        let expected = match kind {
            ModelKind::Voice => "Whisper model (.bin)",
            ModelKind::Language => "language model (.gguf)",
        };
        let mut magic = [0; 4];
        let mut reader = File::open(source)?;
        let readable = reader.read_exact(&mut magic).is_ok();
        if !is_model_file(kind, file) || !readable || magic != file_magic(kind) {
            return Err(ModelsError::WrongFormat { expected });
        }

        let dir = self.dir(kind);
        fs::create_dir_all(&dir)?;
        let destination = self.path(kind, file)?;
        if destination.exists() {
            return Err(ModelsError::AlreadyInstalled {
                file: file.to_owned(),
            });
        }
        let size = fs::metadata(source)?.len();
        crate::download::check_space(&dir, size)?;
        let staging = dir.join(format!("{file}{IMPORT_SUFFIX}"));
        let copied = fs::copy(source, &staging)
            .and_then(|_| File::open(&staging)?.sync_all())
            .and_then(|()| fs::rename(&staging, &destination));
        if let Err(e) = copied {
            let _ = fs::remove_file(&staging);
            return Err(e.into());
        }
        Ok(custom_model(kind, file, size))
    }
}

fn file_magic(kind: ModelKind) -> [u8; 4] {
    match kind {
        // whisper.cpp's `GGML_FILE_MAGIC`, 0x67676d6c, little-endian.
        ModelKind::Voice => *b"lmgg",
        ModelKind::Language => *b"GGUF",
    }
}

fn is_model_file(kind: ModelKind, file: &str) -> bool {
    let extension = match kind {
        ModelKind::Voice => ".bin",
        ModelKind::Language => ".gguf",
    };
    !file.starts_with('.')
        && file.len() > extension.len()
        && file.to_ascii_lowercase().ends_with(extension)
}

fn check_file_name(file: &str) -> Result<()> {
    let plain =
        !file.is_empty() && file != "." && file != ".." && !file.contains(['/', '\\', ':', '\0']);
    if plain {
        Ok(())
    } else {
        Err(ModelsError::InvalidFileName(file.to_owned()))
    }
}

fn file_size(path: &Path) -> Result<Option<u64>> {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() => Ok(Some(meta.len())),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn custom_model(kind: ModelKind, file: &str, size_bytes: u64) -> ModelInfo {
    let label = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let size_mb = u32::try_from(size_bytes.div_ceil(1024 * 1024)).unwrap_or(u32::MAX);
    ModelInfo {
        id: format!("custom/{}/{file}", kind.dir_name()),
        kind,
        label: label.to_owned(),
        description: "Added from your computer.".to_owned(),
        family: "custom".to_owned(),
        file: file.to_owned(),
        url: None,
        size_bytes,
        sha256: None,
        variant_of: None,
        compression: Compression::None,
        english_only: false,
        speed: 0,
        accuracy: 0,
        // Weights plus a working margin, the same rule the catalog follows.
        ram_mb: size_mb.saturating_add(size_mb / 4).saturating_add(200),
        context_length: None,
        reasoning: false,
        recommended: false,
        custom: true,
        status: ModelStatus::Downloaded,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "tw-models-inventory-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn entry(kind: ModelKind, id: &str, file: &str, size: u64) -> ModelInfo {
        let mut model = tw_core::models::catalog()
            .into_iter()
            .find(|m| m.kind == kind)
            .unwrap();
        model.id = id.to_owned();
        model.file = file.to_owned();
        model.size_bytes = size;
        model
    }

    fn write(path: PathBuf, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn paths() {
        let inv = Inventory::new("/data/models");
        assert_eq!(
            inv.path(ModelKind::Voice, "ggml-base.bin").unwrap(),
            PathBuf::from("/data/models/whisper/ggml-base.bin")
        );
        assert_eq!(
            inv.partial_path(ModelKind::Language, "q.gguf").unwrap(),
            PathBuf::from("/data/models/llm/q.gguf.partial")
        );
        for bad in ["", "..", "../x.bin", "a/b.bin", "a\\b.bin"] {
            assert!(matches!(
                inv.path(ModelKind::Voice, bad),
                Err(ModelsError::InvalidFileName(_))
            ));
        }
    }

    #[test]
    fn models_reports_status_from_disk() {
        let tmp = TempDir::new();
        let inv = Inventory::new(&tmp.0);
        let catalog = [
            entry(ModelKind::Voice, "done", "done.bin", 4),
            entry(ModelKind::Voice, "damaged", "damaged.bin", 4),
            entry(ModelKind::Voice, "half", "half.bin", 4),
            entry(ModelKind::Voice, "missing", "missing.bin", 4),
            entry(ModelKind::Language, "llm", "done.bin", 4),
        ];
        write(inv.path(ModelKind::Voice, "done.bin").unwrap(), b"lmgg");
        write(inv.path(ModelKind::Voice, "damaged.bin").unwrap(), b"lm");
        write(
            inv.partial_path(ModelKind::Voice, "half.bin").unwrap(),
            b"lm",
        );
        write(inv.path(ModelKind::Voice, "mine.bin").unwrap(), b"lmgg1234");
        write(inv.path(ModelKind::Voice, "notes.txt").unwrap(), b"x");
        write(inv.path(ModelKind::Voice, ".hidden.bin").unwrap(), b"x");
        write(
            inv.partial_path(ModelKind::Voice, "other.bin").unwrap(),
            b"x",
        );
        write(inv.path(ModelKind::Language, "own.GGUF").unwrap(), b"GGUF");

        let models = inv.models(&catalog).unwrap();
        let status = |id: &str| &models.iter().find(|m| m.id == id).unwrap().status;
        assert_eq!(status("done"), &ModelStatus::Downloaded);
        assert!(matches!(status("damaged"), ModelStatus::Failed { .. }));
        assert_eq!(
            status("half"),
            &ModelStatus::Paused {
                received: 2,
                total: 4
            }
        );
        assert_eq!(status("missing"), &ModelStatus::NotDownloaded);
        assert_eq!(status("llm"), &ModelStatus::NotDownloaded);

        let custom: Vec<_> = models.iter().filter(|m| m.custom).collect();
        assert_eq!(custom.len(), 2, "{custom:#?}");
        assert_eq!(custom[0].id, "custom/whisper/mine.bin");
        assert_eq!(custom[0].label, "mine");
        assert_eq!(custom[0].size_bytes, 8);
        assert_eq!(custom[0].status, ModelStatus::Downloaded);
        assert_eq!(custom[1].kind, ModelKind::Language);
        assert_eq!(custom[1].file, "own.GGUF");
    }

    #[test]
    fn models_without_a_models_dir_is_the_catalog() {
        let tmp = TempDir::new();
        let inv = Inventory::new(tmp.0.join("not-created"));
        let catalog = tw_core::models::catalog();
        assert_eq!(inv.models(&catalog).unwrap(), catalog);
    }

    #[test]
    fn delete_removes_file_and_partial() {
        let tmp = TempDir::new();
        let inv = Inventory::new(&tmp.0);
        let file = inv.path(ModelKind::Language, "a.gguf").unwrap();
        let partial = inv.partial_path(ModelKind::Language, "a.gguf").unwrap();
        write(file.clone(), b"GGUF");
        write(partial.clone(), b"GG");
        inv.delete(ModelKind::Language, "a.gguf").unwrap();
        assert!(!file.exists() && !partial.exists());
        assert!(matches!(
            inv.delete(ModelKind::Language, "a.gguf"),
            Err(ModelsError::NotFound { .. })
        ));
    }

    #[test]
    fn import_checks_the_header() {
        let tmp = TempDir::new();
        let inv = Inventory::new(tmp.0.join("models"));
        let source = tmp.0.join("source");
        write(source.join("mine.gguf"), b"GGUF\x03\0\0\0rest");
        write(source.join("mine.bin"), b"lmgg\x01rest");
        write(source.join("fake.gguf"), b"<!doctype html>");
        write(source.join("tiny.gguf"), b"GG");
        write(source.join("named.txt"), b"GGUF");

        let model = inv
            .import(&source.join("mine.gguf"), ModelKind::Language)
            .unwrap();
        assert_eq!(model.id, "custom/llm/mine.gguf");
        assert!(model.custom);
        assert_eq!(model.size_bytes, 12);
        assert_eq!(
            fs::read(inv.path(ModelKind::Language, "mine.gguf").unwrap()).unwrap(),
            b"GGUF\x03\0\0\0rest"
        );
        inv.import(&source.join("mine.bin"), ModelKind::Voice)
            .unwrap();

        for (file, kind) in [
            ("fake.gguf", ModelKind::Language),
            ("tiny.gguf", ModelKind::Language),
            ("named.txt", ModelKind::Language),
            ("mine.gguf", ModelKind::Voice),
            ("mine.bin", ModelKind::Language),
        ] {
            assert!(
                matches!(
                    inv.import(&source.join(file), kind),
                    Err(ModelsError::WrongFormat { .. })
                ),
                "{file} as {kind:?}"
            );
        }
        assert!(matches!(
            inv.import(&source.join("mine.gguf"), ModelKind::Language),
            Err(ModelsError::AlreadyInstalled { .. })
        ));
        let leftovers: Vec<_> = fs::read_dir(inv.dir(ModelKind::Language))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, ["mine.gguf"]);
    }
}
