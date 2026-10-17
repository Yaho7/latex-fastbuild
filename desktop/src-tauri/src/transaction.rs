use crate::models::{FileChanges, MutationResult, ReferenceChange, ReplacementPlan};
use crate::workspace::{digest, save_atomic, StoredConversion};
use crate::Project;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct Prepared {
    pub plan: ReplacementPlan,
    pub files: Vec<(PathBuf, Vec<u8>, Vec<u8>)>,
}

#[derive(Serialize, Deserialize)]
struct Undo {
    version: u8,
    main_file: String,
    references_changed: usize,
    files: Vec<UndoFile>,
}

#[derive(Serialize, Deserialize)]
struct UndoFile {
    path: String,
    before: String,
    after_hash: String,
}

pub(crate) fn prepare(project: &Project, sources: &BTreeMap<PathBuf, Vec<u8>>, converted: &BTreeMap<String, StoredConversion>) -> Result<Prepared, String> {
    let root = Path::new(&project.root);
    let mut edits: BTreeMap<PathBuf, Vec<(usize, usize, usize, String, String)>> = BTreeMap::new();
    for asset in &project.images {
        let Some(output) = converted.get(&asset.id) else { continue; };
        let original = asset.path.as_ref().ok_or("找不到原图片")?;
        if digest(&fs::read(original).map_err(|e| e.to_string())?) != output.source_hash {
            return Err(format!("图片已被修改，请重新转换：{}", asset.relative_path));
        }
        for (path, hash) in &output.dependencies {
            if digest(&fs::read(path).map_err(|e| e.to_string())?) != *hash { return Err("SVG 的引用资源已修改，请重新转换".into()); }
        }
        let real = output.path.canonicalize().map_err(|e| format!("转换文件已丢失：{e}"))?;
        if !real.starts_with(root) || digest(&fs::read(&real).map_err(|e| e.to_string())?) != output.pdf_hash {
            return Err("转换文件已被修改，请重新转换".into());
        }
        // TeX resolves includegraphics relative to the main compilation directory.
        let relative = pathdiff::diff_paths(&real, root).ok_or("无法生成相对图片路径")?.to_string_lossy().replace('\\', "/");
        if relative.contains(['{', '}', '%', '#', '\n', '\r']) { return Err("输出目录含有 TeX 特殊字符，请调整图片目录名称".into()); }
        for reference in &asset.references {
            edits.entry(PathBuf::from(&reference.file)).or_default().push((reference.start, reference.end, reference.line, reference.original.clone(), relative.clone()));
        }
    }
    let mut plan = ReplacementPlan { files: Vec::new(), reference_count: 0 };
    let mut files = Vec::new();
    for (path, mut changes) in edits {
        let before = sources.get(&path).ok_or("TeX 文件不在扫描记录中")?;
        ensure_current(root, &path, before)?;
        let mut after = String::from_utf8(before.clone()).map_err(|e| e.to_string())?;
        changes.sort_by_key(|entry| entry.0);
        let mut last_end = 0;
        for (start, end, _, original, _) in &changes {
            if start < &last_end || after.get(*start..*end) != Some(original.as_str()) { return Err("引用位置已改变，请重新扫描".into()); }
            last_end = *end;
        }
        for (start, end, _, _, replacement) in changes.iter().rev() { after.replace_range(*start..*end, replacement); }
        let count = changes.len();
        plan.files.push(FileChanges {
            path: path.to_string_lossy().into(),
            changes: changes.into_iter().map(|(_, _, line, before, after)| ReferenceChange { line, before, after }).collect(),
        });
        plan.reference_count += count;
        files.push((path, before.clone(), after.into_bytes()));
    }
    Ok(Prepared { plan, files })
}

pub(crate) fn apply(project: &Project, prepared: Prepared) -> Result<MutationResult, String> {
    if prepared.files.is_empty() { return Err("没有可替换的引用，请先转换图片".into()); }
    let root = Path::new(&project.root);
    let _lock = lock_project(root)?;
    let record = Undo {
        version: 1,
        main_file: project.main_file.clone(),
        references_changed: prepared.plan.reference_count,
        files: prepared.files.iter().map(|(path, before, after)| UndoFile {
            path: path.strip_prefix(root).unwrap().to_string_lossy().into(),
            before: STANDARD.encode(before), after_hash: digest(after),
        }).collect(),
    };
    for (path, before, _) in &prepared.files { ensure_current(root, path, before)?; }
    let undo_path = undo_path(root)?;
    save_atomic(&undo_path, &serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?)?;
    for (path, before, after) in &prepared.files {
        // Recheck immediately before each write. A partial operation retains its recovery record.
        ensure_current(root, path, before).and_then(|_| save_atomic(path, after))
            .map_err(|e| format!("替换未全部完成：{e}。已保留恢复记录，可使用撤销"))?;
    }
    Ok(MutationResult { files_changed: prepared.files.len(), references_changed: prepared.plan.reference_count })
}

pub(crate) fn undo(project: &Project) -> Result<MutationResult, String> {
    let root = Path::new(&project.root);
    let _lock = lock_project(root)?;
    let path = undo_path(root)?;
    let record: Undo = serde_json::from_slice(&fs::read(&path).map_err(|_| "没有可撤销的替换记录")?).map_err(|e| format!("恢复记录损坏：{e}"))?;
    if record.version != 1 || record.main_file != project.main_file { return Err("撤销记录不属于当前主文件".into()); }
    let mut restore = Vec::new();
    for entry in &record.files {
        let file = root.join(&entry.path);
        let real = file.canonicalize().map_err(|e| e.to_string())?;
        if !real.starts_with(root) { return Err("恢复文件位于项目之外".into()); }
        let before = STANDARD.decode(&entry.before).map_err(|e| e.to_string())?;
        let current = fs::read(&real).map_err(|e| e.to_string())?;
        if current == before { continue; }
        if digest(&current) != entry.after_hash { return Err(format!("{} 已被编辑，撤销不会覆盖修改", file.display())); }
        restore.push((real, before, current));
    }
    for (file, before, expected) in &restore {
        ensure_current(root, file, expected)?;
        save_atomic(file, before)?;
    }
    fs::remove_file(path).map_err(|e| e.to_string())?;
    crate::workspace::sync_directory(&root.join(".texgo"))?;
    Ok(MutationResult { files_changed: restore.len(), references_changed: record.references_changed })
}

pub(crate) fn undo_available(root: &Path) -> bool {
    root.join(".texgo/undo.json").is_file()
}

fn undo_path(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join(".texgo");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if !dir.canonicalize().map_err(|e| e.to_string())?.starts_with(root) { return Err("项目状态目录不能链接到项目之外".into()); }
    Ok(dir.join("undo.json"))
}

fn lock_project(root: &Path) -> Result<fs::File, String> {
    let path = undo_path(root)?.with_file_name("write.lock");
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) { return Err("项目锁文件不能是符号链接".into()); }
    let file = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&file).map_err(|_| "另一个 Texgo 实例正在修改项目，请稍后重试".to_string())?;
    Ok(file)
}

fn ensure_current(root: &Path, path: &Path, expected: &[u8]) -> Result<(), String> {
    let real = path.canonicalize().map_err(|e| e.to_string())?;
    if !real.starts_with(root) || real != path { return Err("文件路径发生变化，请重新扫描".into()); }
    if fs::read(&real).map_err(|e| e.to_string())? != expected { return Err(format!("{} 已被编辑，请重新扫描", path.display())); }
    Ok(())
}
