use crate::conversion;
use crate::models::*;
use crate::{scanner, transaction};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Workspace {
    scan: scanner::Scan,
    converted: BTreeMap<String, StoredConversion>,
}

pub(crate) struct StoredConversion {
    pub path: PathBuf,
    pub source_hash: String,
    pub pdf_hash: String,
}

impl Workspace {
    pub fn open(main: &Path) -> Result<Self, String> {
        let mut scan = scanner::scan(main)?;
        scan.project.undo_available = transaction::undo_available(Path::new(&scan.project.root));
        Ok(Self { scan, converted: BTreeMap::new() })
    }

    pub fn project(&self) -> &Project { &self.scan.project }

    pub fn convert(&mut self, options: &ConversionOptions, cancel: &AtomicBool, progress: impl Fn(Progress)) -> Result<ConversionResult, String> {
        conversion::validate(options)?;
        let selected: HashSet<&str> = options.image_ids.iter().map(String::as_str).collect();
        if selected.is_empty() { return Err("请先选择图片".into()); }
        if selected.iter().any(|id| !self.scan.project.images.iter().any(|asset| asset.id == *id && asset.status == "ready")) {
            return Err("所选图片不存在或无法转换，请重新扫描".into());
        }
        let mut images = Vec::new();
        let mut cancelled = false;
        for asset in self.scan.project.images.iter().filter(|asset| selected.contains(asset.id.as_str())) {
            if cancel.load(Ordering::Relaxed) { cancelled = true; break; }
            progress(Progress { current: images.len(), total: selected.len(), name: asset.relative_path.clone(), status: "converting".into() });
            let path = Path::new(asset.path.as_deref().ok_or("图片路径不存在")?);
            let result = (|| {
                let real = path.canonicalize().map_err(|e| e.to_string())?;
                if !real.starts_with(&self.scan.project.root) { return Err("图片路径已移出项目".to_string()); }
                let source_hash = digest(&fs::read(&real).map_err(|e| e.to_string())?);
                let (output, cached) = conversion::convert(&real, options)?;
                if digest(&fs::read(&real).map_err(|e| e.to_string())?) != source_hash { return Err("转换过程中图片发生变化，请重试".into()); }
                let bytes = fs::read(&output).map_err(|e| e.to_string())?;
                let converted = ConvertedImage { id: asset.id.clone(), pdf_path: Some(output.to_string_lossy().into()), bytes: Some(bytes.len() as u64), cached, error: None };
                self.converted.insert(asset.id.clone(), StoredConversion { path: output, source_hash, pdf_hash: digest(&bytes) });
                Ok(converted)
            })();
            let converted = result.unwrap_or_else(|error| {
                self.converted.remove(&asset.id);
                ConvertedImage { id: asset.id.clone(), pdf_path: None, bytes: None, cached: false, error: Some(error) }
            });
            let status = if converted.error.is_some() { "error" } else { "done" };
            images.push(converted);
            progress(Progress { current: images.len(), total: selected.len(), name: asset.relative_path.clone(), status: status.into() });
        }
        Ok(ConversionResult { images, cancelled })
    }

    pub fn plan(&self) -> Result<ReplacementPlan, String> {
        Ok(transaction::prepare(&self.scan.project, &self.scan.sources, &self.converted)?.plan)
    }

    pub fn apply(&mut self) -> Result<MutationResult, String> {
        let prepared = transaction::prepare(&self.scan.project, &self.scan.sources, &self.converted)?;
        let result = transaction::apply(&self.scan.project, prepared);
        self.scan.project.undo_available = transaction::undo_available(Path::new(&self.scan.project.root));
        if result.is_ok() {
            let refreshed = Self::open(Path::new(&self.scan.project.main_file))?;
            *self = refreshed;
        }
        result
    }

    pub fn undo(&mut self) -> Result<MutationResult, String> {
        let result = transaction::undo(&self.scan.project)?;
        *self = Self::open(Path::new(&self.scan.project.main_file))?;
        Ok(result)
    }

    pub fn preview(&self, image_id: &str, converted: bool) -> Result<AssetPreview, String> {
        let asset = self.scan.project.images.iter().find(|asset| asset.id == image_id).ok_or("找不到图片")?;
        let path = if converted {
            self.converted.get(image_id).map(|entry| entry.path.as_path()).ok_or("请先转换此图片")?
        } else { Path::new(asset.path.as_deref().ok_or("图片路径未解析")?) };
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        if !path.starts_with(&self.scan.project.root) { return Err("图片位于项目之外".into()); }
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 { return Err("文件过大，无法内嵌预览".into()); }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) {
            return Ok(AssetPreview { data: STANDARD.encode(bytes), kind: "pdf".into() });
        }
        // Decode raster formats so TIFF/BMP preview consistently in every WebView.
        if asset.format == "svg" {
            return Ok(AssetPreview { data: format!("data:image/svg+xml;base64,{}", STANDARD.encode(bytes)), kind: "image".into() });
        }
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().map_err(|e| e.to_string())?.thumbnail(1600, 1600);
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok(AssetPreview { data: format!("data:image/png;base64,{}", STANDARD.encode(png.into_inner())), kind: "image".into() })
    }
}

pub(crate) fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

pub(crate) fn save_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("文件目录不存在")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    if let Ok(metadata) = fs::metadata(path) { file.as_file().set_permissions(metadata.permissions()).map_err(|e| e.to_string())?; }
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
