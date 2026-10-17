use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub main_file: String,
    pub root: String,
    pub tex_files: Vec<String>,
    pub images: Vec<ImageAsset>,
    pub warnings: Vec<String>,
    pub undo_available: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAsset {
    pub id: String,
    pub path: Option<String>,
    pub relative_path: String,
    pub format: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bytes: Option<u64>,
    pub status: String,
    pub message: Option<String>,
    pub references: Vec<Reference>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub file: String,
    pub line: usize,
    pub original: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionOptions {
    pub image_ids: Vec<String>,
    pub quality: String,
    pub dpi: u32,
    pub max_width_mm: f64,
    pub jpeg_quality: u8,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertedImage {
    pub id: String,
    pub pdf_path: Option<String>,
    pub bytes: Option<u64>,
    pub cached: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionResult {
    pub images: Vec<ConvertedImage>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub current: usize,
    pub total: usize,
    pub name: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacementPlan {
    pub files: Vec<FileChanges>,
    pub reference_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChanges {
    pub path: String,
    pub changes: Vec<ReferenceChange>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceChange {
    pub line: usize,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationResult {
    pub files_changed: usize,
    pub references_changed: usize,
}

#[derive(Serialize)]
pub struct AssetPreview {
    pub data: String,
    pub kind: String,
}
