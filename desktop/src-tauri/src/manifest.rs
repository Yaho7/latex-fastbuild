use crate::models::{ImageAsset, Project};
use crate::workspace::{digest, save_atomic};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Default, Serialize, Deserialize)]
struct Manifest {
    version: u8,
    images: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    source: String,
}

pub(crate) fn remember(root: &Path, source: &Path, output: &Path) -> Result<(), String> {
    let _lock = crate::transaction::lock_project(root)?;
    let path = root.join(".texgo/images.json");
    let mut manifest: Manifest = if path.exists() {
        serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| format!("图片映射记录损坏：{e}"))?
    } else { Manifest { version: 1, images: BTreeMap::new() } };
    if manifest.version != 1 { return Err("不支持的图片映射记录版本".into()); }
    let source = source.strip_prefix(root).map_err(|_| "原图片位于项目之外")?.to_string_lossy().replace('\\', "/");
    let output = output.strip_prefix(root).map_err(|_| "转换结果位于项目之外")?.to_string_lossy().replace('\\', "/");
    manifest.images.insert(output, Entry { source });
    save_atomic(&path, &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)
}

pub(crate) fn restore_sources(project: &mut Project) {
    let root = std::path::PathBuf::from(&project.root);
    let path = root.join(".texgo/images.json");
    if !path.exists() { return; }
    if !path.canonicalize().is_ok_and(|path| path.starts_with(&root)) {
        project.warnings.push("图片映射记录位于项目之外，已忽略".into());
        return;
    }
    let manifest = fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok());
    let Some(manifest) = manifest.filter(|manifest| manifest.version == 1) else {
        project.warnings.push("图片映射记录不可读取，已有 PDF 将按独立图片处理".into());
        return;
    };
    let mut assets: Vec<ImageAsset> = Vec::new();
    for mut asset in std::mem::take(&mut project.images) {
        if asset.status == "pdf" || (asset.status == "missing" && asset.format == "pdf") {
            if let Some(entry) = manifest.images.get(&asset.relative_path) {
                let source = root.join(&entry.source).canonicalize().ok().filter(|path| path.starts_with(&root) && path.is_file());
                if let Some(source) = source {
                    let format = source.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_lowercase();
                    if matches!(format.as_str(), "png" | "jpg" | "jpeg" | "svg" | "webp" | "tif" | "tiff" | "bmp" | "gif") {
                        asset.id = digest(source.to_string_lossy().as_bytes());
                        asset.path = Some(source.to_string_lossy().into());
                        asset.relative_path = entry.source.clone();
                        asset.format = format;
                        let dimensions = image::image_dimensions(&source).ok();
                        asset.width = dimensions.map(|size| size.0);
                        asset.height = dimensions.map(|size| size.1);
                        asset.bytes = fs::metadata(&source).ok().map(|metadata| metadata.len());
                        asset.status = "ready".into();
                        asset.message = Some("已关联原图片，重新转换可更新 PDF".into());
                    }
                } else {
                    project.warnings.push(format!("转换 PDF 的原图片已丢失：{}", entry.source));
                    asset.message = Some("已找不到原图片，保留现有 PDF".into());
                }
            }
        }
        if let Some(existing) = assets.iter_mut().find(|existing| existing.id == asset.id) {
            existing.references.extend(asset.references);
        } else { assets.push(asset); }
    }
    project.images = assets;
}
