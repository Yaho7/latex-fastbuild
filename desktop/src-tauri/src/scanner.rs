use crate::models::{ImageAsset, Project, Reference};
use crate::workspace::digest;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const EXTENSIONS: &[&str] = &["pdf", "png", "jpg", "jpeg", "svg", "webp", "tif", "tiff", "bmp", "gif", "eps", "ps"];

pub(crate) struct Scan {
    pub project: Project,
    pub sources: BTreeMap<PathBuf, Vec<u8>>,
}

struct Scanner {
    root: PathBuf,
    project: Project,
    sources: BTreeMap<PathBuf, Vec<u8>>,
    indices: HashMap<String, usize>,
    active: HashSet<PathBuf>,
    graphics_dirs: Vec<PathBuf>,
    contexts: HashMap<PathBuf, Vec<PathBuf>>,
    unsafe_context: bool,
}

struct Command {
    name: String,
    start: usize,
    end: usize,
    line: usize,
    depth: usize,
}

pub(crate) fn scan(main: &Path) -> Result<Scan, String> {
    let main = main.canonicalize().map_err(|e| format!("无法打开主文件：{e}"))?;
    if main.extension().and_then(|s| s.to_str()) != Some("tex") {
        return Err("请选择 .tex 主文件".into());
    }
    let root = main.parent().ok_or("主文件没有父目录")?.to_path_buf();
    let mut scanner = Scanner {
        project: Project {
            main_file: main.to_string_lossy().into(),
            root: root.to_string_lossy().into(),
            tex_files: Vec::new(), images: Vec::new(), warnings: Vec::new(), undo_available: false,
        },
        root, sources: BTreeMap::new(), indices: HashMap::new(), active: HashSet::new(), graphics_dirs: Vec::new(),
        contexts: HashMap::new(), unsafe_context: false,
    };
    scanner.visit(&main)?;
    if scanner.unsafe_context {
        scanner.project.warnings.push("检测到局部图片搜索路径或不同搜索路径下的重复子文件引用；请改用明确的图片路径后重新扫描".into());
        for asset in &mut scanner.project.images {
            if asset.status == "ready" {
                asset.status = "unsupported".into();
                asset.message = Some("图片搜索路径依赖 TeX 上下文，无法安全自动替换".into());
            }
        }
    }
    Ok(Scan { project: scanner.project, sources: scanner.sources })
}

impl Scanner {
    fn visit(&mut self, file: &Path) -> Result<(), String> {
        if self.active.contains(file) {
            self.project.warnings.push(format!("跳过循环引用：{}", file.display()));
            return Ok(());
        }
        if self.sources.contains_key(file) {
            if self.contexts.get(file) != Some(&self.graphics_dirs) { self.unsafe_context = true; }
            return Ok(());
        }
        if self.sources.len() >= 1024 { return Err("项目包含超过 1024 个 TeX 文件，请缩小项目范围".into()); }
        let bytes = fs::read(file).map_err(|e| format!("读取 {} 失败：{e}", file.display()))?;
        if bytes.len() > 16 * 1024 * 1024 { return Err(format!("TeX 文件过大：{}", file.display())); }
        let text = String::from_utf8(bytes.clone()).map_err(|_| format!("{} 不是 UTF-8 文件，请先转换编码", file.display()))?;
        let masked = mask_non_code(&text);
        self.sources.insert(file.to_path_buf(), bytes);
        self.contexts.insert(file.to_path_buf(), self.graphics_dirs.clone());
        self.project.tex_files.push(file.to_string_lossy().into());
        self.active.insert(file.to_path_buf());
        for command in commands(&masked) {
            let arg = masked[command.start..command.end].trim();
            match command.name.as_str() {
                "input" | "include" | "subfile" => {
                    if dynamic(arg) {
                        self.project.warnings.push(format!("{}:{}：无法静态解析子文件 {arg}", file.display(), command.line));
                        continue;
                    }
                    let mut path = PathBuf::from(arg);
                    if path.extension().is_none() { path.set_extension("tex"); }
                    let resolved = [self.root.join(&path), file.parent().unwrap().join(&path)]
                        .into_iter().find_map(|p| p.canonicalize().ok());
                    match resolved {
                        Some(path) if path.starts_with(&self.root) => self.visit(&path)?,
                        _ => self.project.warnings.push(format!("{}:{}：子文件不存在或位于项目之外：{arg}", file.display(), command.line)),
                    }
                }
                "graphicspath" => {
                    if command.depth > 0 { self.unsafe_context = true; }
                    let mut i = command.start;
                    let mut dirs = Vec::new();
                    while i < command.end {
                        i = skip_space(masked.as_bytes(), i);
                        if let Some((start, end)) = group(masked.as_bytes(), i, b'{', b'}') {
                            let dir = masked[start..end].trim();
                            if !dynamic(dir) { dirs.push(self.root.join(dir)); }
                            else { self.project.warnings.push(format!("无法静态解析图片目录：{dir}")); }
                            i = end + 1;
                        } else { break; }
                    }
                    self.graphics_dirs = dirs;
                }
                "includegraphics" => self.add_image(file, &text, &command, arg),
                "DeclareGraphicsExtensions" => self.project.warnings.push("项目声明了自定义图片扩展名顺序；省略扩展名的多重匹配需手动确认".into()),
                _ => (),
            }
        }
        self.active.remove(file);
        Ok(())
    }

    fn add_image(&mut self, file: &Path, source: &str, command: &Command, arg: &str) {
        let reference = Reference {
            file: file.to_string_lossy().into(), line: command.line,
            original: source[command.start..command.end].to_string(), start: command.start, end: command.end,
        };
        let mut candidates = Vec::new();
        let mut status = "ready";
        let mut message = None;
        if dynamic(arg) || arg.is_empty() || arg.contains(['\n', '\r']) {
            status = "unsupported";
            message = Some("引用包含宏、条件或多行路径，需要手动确认".into());
        } else {
            let bases = std::iter::once(self.root.clone()).chain(self.graphics_dirs.clone()).chain(std::iter::once(file.parent().unwrap().to_path_buf()));
            for base in bases {
                let path = base.join(arg);
                let paths = if path.extension().is_some() { vec![path] }
                    else { EXTENSIONS.iter().map(|ext| path.with_extension(ext)).collect() };
                for candidate in paths {
                    if let Ok(real) = candidate.canonicalize() {
                        if real.is_file() && real.starts_with(&self.root) && !candidates.contains(&real) { candidates.push(real); }
                    }
                }
                if !candidates.is_empty() { break; }
            }
            if candidates.is_empty() { status = "missing"; message = Some("找不到图片，或图片位于项目目录之外".into()); }
            else if candidates.len() > 1 { status = "ambiguous"; message = Some("找到多个同名图片，请在 TeX 中明确扩展名后重新扫描".into()); }
        }
        let path = if candidates.len() == 1 { candidates.pop() } else { None };
        let key = path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| format!("{}:{arg}", file.display()));
        if let Some(&index) = self.indices.get(&key) {
            self.project.images[index].references.push(reference);
            return;
        }
        let format = path.as_deref().unwrap_or_else(|| Path::new(arg)).extension()
            .and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
        if status == "ready" {
            if format == "pdf" { status = "pdf"; }
            else if !EXTENSIONS.contains(&format.as_str()) || matches!(format.as_str(), "eps" | "ps") {
                status = "unsupported"; message = Some("此格式暂不支持转换，请先导出为 SVG、PNG 或 JPEG".into());
            }
        }
        let dimensions = path.as_ref().and_then(|p| image::image_dimensions(p).ok());
        let bytes = path.as_ref().and_then(|p| fs::metadata(p).ok()).map(|m| m.len());
        let relative_path = path.as_ref().and_then(|p| p.strip_prefix(&self.root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|| arg.to_string());
        self.indices.insert(key.clone(), self.project.images.len());
        self.project.images.push(ImageAsset {
            id: digest(key.as_bytes()), path: path.map(|p| p.to_string_lossy().into()), relative_path,
            format, width: dimensions.map(|d| d.0), height: dimensions.map(|d| d.1), bytes,
            status: status.into(), message, references: vec![reference],
        });
    }
}

fn dynamic(value: &str) -> bool { value.contains(['\\', '#', '{', '}', '~', '$']) }
fn skip_space(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
    i
}

fn group(bytes: &[u8], at: usize, open: u8, close: u8) -> Option<(usize, usize)> {
    if bytes.get(at) != Some(&open) { return None; }
    let mut depth = 1;
    let mut i = at + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' { i += 2; continue; }
        if bytes[i] == open { depth += 1; }
        if bytes[i] == close { depth -= 1; if depth == 0 { return Some((at + 1, i)); } }
        i += 1;
    }
    None
}

fn commands(text: &str) -> Vec<Command> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut depth: usize = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' { depth += 1; i += 1; continue; }
        if bytes[i] == b'}' { depth = depth.saturating_sub(1); i += 1; continue; }
        if bytes[i] != b'\\' { i += 1; continue; }
        let command_at = i;
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() { i += 1; }
        if start == i { i += 1; continue; }
        let name = &text[start..i];
        if matches!(name, "begingroup" | "bgroup") { depth += 1; continue; }
        if matches!(name, "endgroup" | "egroup") { depth = depth.saturating_sub(1); continue; }
        if matches!(name, "begin" | "end") {
            if let Some((start, end)) = group(bytes, skip_space(bytes, i), b'{', b'}') {
                if &text[start..end] != "document" {
                    if name == "begin" { depth += 1; } else { depth = depth.saturating_sub(1); }
                }
                i = end + 1;
            }
            continue;
        }
        if !matches!(name, "input" | "include" | "subfile" | "includegraphics" | "graphicspath" | "DeclareGraphicsExtensions") { continue; }
        if bytes.get(i) == Some(&b'*') { i += 1; }
        i = skip_space(bytes, i);
        while bytes.get(i) == Some(&b'[') {
            if let Some((_, end)) = group(bytes, i, b'[', b']') { i = skip_space(bytes, end + 1); }
            else { break; }
        }
        if let Some((start, end)) = group(bytes, i, b'{', b'}') {
            out.push(Command { name: name.into(), start, end, depth, line: bytes[..command_at].iter().filter(|&&b| b == b'\n').count() + 1 });
            i = end + 1;
        }
    }
    out
}

// Mask without changing byte positions so replacements preserve the original source.
fn mask_non_code(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut masked = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if bytes[i] == b'%' {
            while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
        } else if bytes[i] == b'\\' {
            let mut skipped = false;
            for env in ["verbatim", "verbatim*", "Verbatim", "lstlisting", "minted"] {
                let begin = format!("\\begin{{{env}}}");
                if bytes[i..].starts_with(begin.as_bytes()) {
                    let end = format!("\\end{{{env}}}");
                    i = text[i + begin.len()..].find(&end).map(|p| i + begin.len() + p + end.len()).unwrap_or(bytes.len());
                    skipped = true; break;
                }
            }
            if !skipped && bytes[i..].starts_with(b"\\verb") && bytes.get(i + 5).is_some_and(|b| !b.is_ascii_alphabetic()) {
                let mut delimiter = i + 5;
                if bytes.get(delimiter) == Some(&b'*') { delimiter += 1; }
                if let Some(value) = text.get(delimiter..).and_then(|s| s.chars().next()) {
                    i = delimiter + value.len_utf8();
                    while i < bytes.len() {
                        let current = text[i..].chars().next().unwrap();
                        i += current.len_utf8();
                        if current == value || current == '\n' { break; }
                    }
                    skipped = true;
                }
            }
            if !skipped { i += 2; continue; }
        } else { i += 1; continue; }
        for byte in &mut masked[start..i] { if *byte != b'\n' && *byte != b'\r' { *byte = b' '; } }
    }
    String::from_utf8(masked).expect("mask preserves UTF-8")
}
