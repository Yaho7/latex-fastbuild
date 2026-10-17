//! Headless, local automation shared by the CLI and newline-delimited JSON-RPC.
use crate::{ConversionOptions, ConversionResult, MutationResult, Project, Workspace};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime};

pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEBOUNCE: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub version: u32,
    pub main_file: PathBuf,
    #[serde(default)]
    pub images: ImageConfig,
    #[serde(default)]
    pub build: BuildConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ImageConfig {
    pub enabled: bool,
    pub quality: String,
    pub dpi: u32,
    pub max_width_mm: f64,
    pub jpeg_quality: u8,
    pub replace: bool,
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self { enabled: true, quality: "original".into(), dpi: 300,
            max_width_mm: 160.0, jpeg_quality: 90, replace: false }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct BuildConfig {
    pub engine: String,
    pub output_dir: PathBuf,
    pub synctex: bool,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self { engine: "xelatex".into(), output_dir: "build".into(), synctex: true }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedConfig {
    pub config: ProjectConfig,
    pub config_path: PathBuf,
    pub main_file: PathBuf,
    pub root: PathBuf,
    pub output_dir: PathBuf,
}

fn validate_options(options: &ConversionOptions) -> Result<(), String> {
    if !matches!(options.quality.as_str(), "original" | "high" | "compact" | "custom") {
        return Err("quality must be original, high, compact, or custom".into());
    }
    if !(72..=1200).contains(&options.dpi)
        || !options.max_width_mm.is_finite() || !(10.0..=1000.0).contains(&options.max_width_mm)
        || !(1..=100).contains(&options.jpeg_quality) {
        return Err("image options require dpi 72..1200, maxWidthMm 10..1000, jpegQuality 1..100".into());
    }
    Ok(())
}

impl ImageConfig {
    fn options(&self, image_ids: Vec<String>) -> ConversionOptions {
        ConversionOptions { image_ids, quality: self.quality.clone(), dpi: self.dpi,
            max_width_mm: self.max_width_mm, jpeg_quality: self.jpeg_quality }
    }
}

/// mainFile is relative to the config; outputDir is relative to mainFile's parent.
pub fn load_config(path: &Path) -> Result<ResolvedConfig, String> {
    let config_path = path.canonicalize().map_err(|e| format!("Cannot open config {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    fs::File::open(&config_path).map_err(|e| e.to_string())?
        .take((MAX_REQUEST_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_REQUEST_BYTES { return Err("Config exceeds 1 MiB".into()); }
    let config: ProjectConfig = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid project config: {e}"))?;
    if config.version != 1 { return Err("Only project config version 1 is supported".into()); }
    if config.main_file.as_os_str().is_empty() { return Err("mainFile must not be empty".into()); }
    validate_options(&config.images.options(Vec::new()))?;
    if !matches!(config.build.engine.as_str(), "xelatex" | "lualatex" | "pdflatex") {
        return Err("build.engine must be xelatex, lualatex, or pdflatex".into());
    }
    let main_file = config_path.parent().ok_or("Config has no parent")?.join(&config.main_file)
        .canonicalize().map_err(|e| format!("Cannot open mainFile: {e}"))?;
    if !main_file.is_file() || main_file.extension().and_then(|s| s.to_str()) != Some("tex") {
        return Err("mainFile must be an existing .tex file".into());
    }
    if main_file.file_name().is_some_and(|s| s.to_string_lossy().starts_with('-')) {
        return Err("mainFile basename must not start with '-'".into());
    }
    let root = main_file.parent().ok_or("mainFile has no parent")?.to_path_buf();
    let output = &config.build.output_dir;
    if output.as_os_str().is_empty() { return Err("build.outputDir must not be empty".into()); }
    let output_dir = if output.is_absolute() { output.clone() } else { root.join(output) };
    let relative = output_dir.strip_prefix(&root).map_err(|_| "build.outputDir must be inside the project")?;
    let mut clean = root.clone();
    let mut count = 0;
    for part in relative.components() {
        match part {
            Component::CurDir => (),
            Component::Normal(name) => {
                if excluded_name(name) { return Err("build.outputDir cannot use a reserved directory".into()); }
                clean.push(name);
                count += 1;
                if let Ok(metadata) = fs::symlink_metadata(&clean) {
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        return Err("build.outputDir must contain only real directories, not symlinks or files".into());
                    }
                }
            }
            _ => return Err("build.outputDir must be a project subdirectory without '..'".into()),
        }
    }
    if count == 0 { return Err("build.outputDir cannot be the project root".into()); }
    let resolved = ResolvedConfig { config, config_path, main_file, root, output_dir: clean };
    protect_sources(&resolved)?;
    Ok(resolved)
}

fn protect_sources(config: &ResolvedConfig) -> Result<(), String> {
    let workspace = Workspace::open(&config.main_file)?;
    for path in workspace.project().tex_files.iter().map(PathBuf::from)
        .chain(workspace.project().images.iter().filter_map(|image| image.path.as_ref().map(PathBuf::from))) {
        if path.starts_with(&config.output_dir) {
            return Err(format!("build.outputDir contains a project source: {}", path.display()));
        }
    }
    // Refuse unrelated source material in the artifact directory as well.
    let mut pending = vec![config.output_dir.clone()];
    let mut visited = 0usize;
    while let Some(directory) = pending.pop() {
        if !directory.exists() { continue; }
        for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            visited += 1;
            if visited > 50_000 { return Err("Too many files in build.outputDir".into()); }
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() { return Err("Symlinks are not allowed in build.outputDir".into()); }
            if kind.is_dir() { pending.push(entry.path()); }
            else if source_extension(&entry.path(), false) {
                return Err(format!("build.outputDir contains source material: {}", entry.path().display()));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub file: Option<String>,
    pub line: Option<usize>,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub pdf_path: Option<String>,
    pub log_path: String,
    pub diagnostics: Vec<Diagnostic>,
}

fn diagnostic(main: &Path, line: Option<usize>, message: &str, severity: &str) -> Diagnostic {
    Diagnostic { file: Some(main.to_string_lossy().into()), line,
        message: message.trim().into(), severity: severity.into() }
}

fn file_line_error(text: &str) -> Option<(&str, usize, &str)> {
    for (index, _) in text.match_indices(':') {
        let remainder = &text[index + 1..];
        let Some(end) = remainder.find(':') else { continue; };
        let number = &remainder[..end];
        if number.is_empty() || !number.bytes().all(|c| c.is_ascii_digit()) { continue; }
        let file = &text[..index];
        if file.ends_with(".tex") || file.ends_with(".sty") || file.ends_with(".cls") || file.ends_with(".bib") {
            return Some((file, number.parse().ok()?, &remainder[end + 1..]));
        }
    }
    None
}

pub fn parse_diagnostics(text: &str, main: &Path) -> Vec<Diagnostic> {
    let mut result: Vec<Diagnostic> = Vec::new();
    let mut pending_error: Option<usize> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some((file, number, message)) = file_line_error(line) {
            let path = Path::new(file);
            let path = if path.is_absolute() { path.to_path_buf() } else { main.parent().unwrap_or(Path::new(".")).join(path) };
            let severity = if message.to_lowercase().contains("warning") { "warning" } else { "error" };
            result.push(Diagnostic { file: Some(path.to_string_lossy().into()), line: Some(number), message: message.trim().into(), severity: severity.into() });
            pending_error = if severity == "error" { Some(result.len() - 1) } else { None };
        } else if let Some(message) = line.strip_prefix('!') {
            result.push(diagnostic(main, None, message, "error"));
            pending_error = Some(result.len() - 1);
        } else if let Some(rest) = line.strip_prefix("l.") {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let (Some(index), Ok(number)) = (pending_error.take(), digits.parse::<usize>()) {
                result[index].line = Some(number);
            }
        } else if line.contains("Warning:") || line.starts_with("Overfull ") || line.starts_with("Underfull ") {
            let number = line.split("input line ").nth(1).and_then(|tail| {
                tail.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
            });
            result.push(diagnostic(main, number, line, "warning"));
        } else if line.starts_with("Latexmk:") {
            let lower = line.to_lowercase();
            if lower.contains("error") || lower.contains("fail") || lower.contains("missing") {
                result.push(diagnostic(main, None, line, "error"));
            }
        }
    }
    result
}

/// Invokes installed latexmk directly. No shell, download, or TeX installation.
pub fn build_document(config: &ResolvedConfig) -> Result<BuildResult, String> {
    // Revalidate before writing even when a caller retains a resolved config.
    let config = load_config(&config.config_path)?;
    let _lock = crate::transaction::lock_project(&config.root)?;
    fs::create_dir_all(&config.output_dir).map_err(|e| format!("Cannot create build.outputDir: {e}"))?;
    if config.output_dir.canonicalize().map_err(|e| e.to_string())? != config.output_dir {
        return Err("build.outputDir changed to a symlink".into());
    }
    let stem = config.main_file.file_stem().ok_or("mainFile has no stem")?.to_string_lossy();
    let log_path = config.output_dir.join(format!("{stem}.texgo-output.log"));
    let pdf_path = config.output_dir.join(format!("{stem}.pdf"));
    let output = Command::new("latexmk")
        .arg(match config.config.build.engine.as_str() { "xelatex" => "-xelatex", "lualatex" => "-lualatex", _ => "-pdf" })
        .arg(if config.config.build.synctex { "-synctex=1" } else { "-synctex=0" })
        .args(["-file-line-error", "-interaction=nonstopmode", "-halt-on-error"])
        .arg(format!("-outdir={}", config.output_dir.display()))
        .arg(config.main_file.file_name().ok_or("mainFile has no basename")?)
        .current_dir(&config.root).output();
    let (exit_code, exited_ok, captured) = match output {
        Ok(output) => {
            let mut captured = output.stdout;
            if !captured.is_empty() && !captured.ends_with(b"\n") { captured.push(b'\n'); }
            captured.extend_from_slice(&output.stderr);
            (output.status.code(), output.status.success(), captured)
        }
        Err(error) => (None, false, format!("Cannot execute installed latexmk: {error}\nInstall a TeX distribution with latexmk and put it on PATH.\n").into_bytes()),
    };
    crate::workspace::save_atomic(&log_path, &captured)
        .map_err(|e| format!("Cannot save full compiler output {}: {e}", log_path.display()))?;
    let text = String::from_utf8_lossy(&captured);
    let mut diagnostics = parse_diagnostics(&text, &config.main_file);
    let success = exited_ok && pdf_path.is_file();
    if !success && !diagnostics.iter().any(|item| item.severity == "error") {
        let message = if exited_ok { "latexmk exited successfully but did not produce a PDF" }
            else { text.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("latexmk failed without output") };
        diagnostics.push(diagnostic(&config.main_file, None, message, "error"));
    }
    Ok(BuildResult { success, exit_code,
        pdf_path: success.then(|| pdf_path.to_string_lossy().into()),
        log_path: log_path.to_string_lossy().into(), diagnostics })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineResult {
    pub project: Project,
    pub conversion: Option<ConversionResult>,
    pub replacement: Option<MutationResult>,
    pub build: BuildResult,
}

pub fn run_pipeline(config: &ResolvedConfig) -> Result<PipelineResult, String> {
    let config = load_config(&config.config_path)?;
    let mut workspace = Workspace::open(&config.main_file)?;
    // Static-analysis uncertainty must never silently mutate an automated project.
    if !workspace.project().warnings.is_empty() {
        return Err(format!("Pipeline requires an unambiguous scan: {}", workspace.project().warnings.join("; ")));
    }
    let unresolved: Vec<_> = workspace.project().images.iter()
        .filter(|asset| asset.status != "ready" && asset.status != "pdf")
        .map(|asset| format!("{} ({})", asset.relative_path, asset.status)).collect();
    if !unresolved.is_empty() {
        return Err(format!("Pipeline cannot resolve all image references: {}", unresolved.join(", ")));
    }
    let mut conversion = None;
    let mut replacement = None;
    let ids = workspace.project().images.iter().filter(|asset| asset.status == "ready")
        .map(|asset| asset.id.clone()).collect::<Vec<_>>();
    if config.config.images.enabled && !ids.is_empty() {
        let result = workspace.convert(&config.config.images.options(ids), &AtomicBool::new(false), |_| {})?;
        let failures = result.images.iter().filter_map(|image| image.error.as_ref()).cloned().collect::<Vec<_>>();
        if result.cancelled || !failures.is_empty() {
            return Err(format!("Image conversion failed; compilation was not started: {}", failures.join("; ")));
        }
        conversion = Some(result);
        if config.config.images.replace && workspace.plan()?.reference_count > 0 {
            replacement = Some(workspace.apply()?);
        }
    }
    let build = build_document(&config)?;
    Ok(PipelineResult { project: workspace.project().clone(), conversion, replacement, build })
}

#[derive(Debug)]
struct RpcError { code: i32, message: String }

impl RpcError {
    fn params(message: impl Into<String>) -> Self { Self { code: -32602, message: message.into() } }
    fn runtime(message: impl Into<String>) -> Self { Self { code: -32000, message: message.into() } }
}

fn rpc_error(id: Value, code: i32, message: impl Into<String>) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message.into()}})
}

fn params<T: DeserializeOwned>(value: Value) -> Result<T, RpcError> {
    if !value.is_object() { return Err(RpcError::params("params must be an object")); }
    serde_json::from_value(value).map_err(|e| RpcError::params(e.to_string()))
}

fn serialize<T: Serialize>(value: T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::runtime(e.to_string()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyParams {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenParams { main_file: PathBuf }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScanParams { #[serde(default)] main_file: Option<PathBuf> }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmParams { confirm: bool }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigParams { config_path: PathBuf }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConvertParams { options: StrictConversionOptions }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StrictConversionOptions {
    image_ids: Vec<String>, quality: String, dpi: u32, max_width_mm: f64, jpeg_quality: u8,
}

/// One session owns one Workspace so conversion plans survive between requests.
#[derive(Default)]
pub struct RpcSession { workspace: Option<Workspace> }

impl RpcSession {
    fn workspace(&mut self) -> Result<&mut Workspace, RpcError> {
        self.workspace.as_mut().ok_or_else(|| RpcError::runtime("Open a project first with project.open"))
    }

    pub fn handle_line(&mut self, line: &str) -> Option<Value> {
        if line.len() > MAX_REQUEST_BYTES {
            return Some(rpc_error(Value::Null, -32600, "Request exceeds 1 MiB"));
        }
        let request: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => return Some(rpc_error(Value::Null, -32700, format!("Parse error: {error}"))),
        };
        let Some(object) = request.as_object() else {
            return Some(rpc_error(Value::Null, -32600, "Expected one JSON-RPC request object (no batches)"));
        };
        let id = object.get("id").cloned().unwrap_or(Value::Null);
        let valid_id = id.is_null() || id.is_string() || id.is_number();
        if request.get("jsonrpc") != Some(&json!("2.0")) || !valid_id || !request.get("method").is_some_and(Value::is_string) {
            return Some(rpc_error(if valid_id { id } else { Value::Null }, -32600, "Invalid JSON-RPC 2.0 envelope"));
        }
        let notification = !object.contains_key("id");
        let method = request["method"].as_str().unwrap_or_default();
        let result = self.dispatch(method, object.get("params").cloned().unwrap_or_else(|| json!({})));
        if notification { return None; }
        Some(match result {
            Ok(result) => json!({"jsonrpc":"2.0", "id":id, "result":result}),
            Err(error) => rpc_error(id, error.code, error.message),
        })
    }

    fn dispatch(&mut self, method: &str, value: Value) -> Result<Value, RpcError> {
        match method {
            "tools.info" => {
                let _: EmptyParams = params(value)?;
                Ok(json!({"name":"texgo-engine", "version":env!("CARGO_PKG_VERSION"),
                    "configVersion":1, "transport":"ndjson-stdio", "maxRequestBytes":MAX_REQUEST_BYTES,
                    "methods":["tools.info","project.open","project.scan","images.convert","references.preview",
                        "references.apply","references.undo","document.build","pipeline.run"],
                    "engines":["xelatex","lualatex","pdflatex"], "compiler":"latexmk"}))
            }
            "project.open" => {
                let input: OpenParams = params(value)?;
                let workspace = Workspace::open(&input.main_file).map_err(RpcError::runtime)?;
                let result = serialize(workspace.project())?;
                self.workspace = Some(workspace);
                Ok(result)
            }
            "project.scan" => {
                let input: ScanParams = params(value)?;
                let main = match input.main_file {
                    Some(main) => main,
                    None => PathBuf::from(&self.workspace()?.project().main_file),
                };
                let workspace = Workspace::open(&main).map_err(RpcError::runtime)?;
                let result = serialize(workspace.project())?;
                self.workspace = Some(workspace);
                Ok(result)
            }
            "images.convert" => {
                let input: ConvertParams = params(value)?;
                let options = ConversionOptions { image_ids: input.options.image_ids, quality: input.options.quality,
                    dpi: input.options.dpi, max_width_mm: input.options.max_width_mm, jpeg_quality: input.options.jpeg_quality };
                validate_options(&options).map_err(RpcError::params)?;
                if options.image_ids.is_empty() { return Err(RpcError::params("options.imageIds must not be empty")); }
                let workspace = self.workspace()?;
                if options.image_ids.iter().any(|id| !workspace.project().images.iter().any(|asset| asset.id == *id && asset.status == "ready")) {
                    return Err(RpcError::params("options.imageIds must select ready images from the current scan"));
                }
                serialize(workspace.convert(&options, &AtomicBool::new(false), |_| {}).map_err(RpcError::runtime)?)
            }
            "references.preview" => {
                let _: EmptyParams = params(value)?;
                serialize(self.workspace()?.plan().map_err(RpcError::runtime)?)
            }
            "references.apply" | "references.undo" => {
                let input: ConfirmParams = params(value)?;
                if !input.confirm { return Err(RpcError::params("Reference mutation requires confirm:true")); }
                let workspace = self.workspace()?;
                serialize(if method == "references.apply" { workspace.apply() } else { workspace.undo() }.map_err(RpcError::runtime)?)
            }
            "document.build" | "pipeline.run" => {
                let input: ConfigParams = params(value)?;
                let config = load_config(&input.config_path).map_err(RpcError::params)?;
                if method == "document.build" { serialize(build_document(&config).map_err(RpcError::runtime)?) }
                else {
                    let result = run_pipeline(&config).map_err(RpcError::runtime)?;
                    // Pipeline may change sources; do not retain stale conversion plans.
                    if self.workspace.as_ref().is_some_and(|ws| Path::new(&ws.project().main_file) == config.main_file) {
                        self.workspace = Some(Workspace::open(&config.main_file).map_err(RpcError::runtime)?);
                    }
                    serialize(result)
                }
            }
            _ => Err(RpcError { code: -32601, message: format!("Method not found: {method}") }),
        }
    }
}

fn write_json(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, value).map_err(io::Error::other)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// Drains oversized frames without allocating their entire contents, then recovers.
pub fn serve_rpc(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut session = RpcSession::default();
    loop {
        let mut frame = Vec::new();
        let mut oversized = false;
        let mut saw_bytes = false;
        loop {
            let available = input.fill_buf()?;
            if available.is_empty() { break; }
            saw_bytes = true;
            let end = available.iter().position(|byte| *byte == b'\n');
            let count = end.map_or(available.len(), |index| index + 1);
            let payload = end.unwrap_or(available.len());
            if !oversized {
                if frame.len() + payload > MAX_REQUEST_BYTES { oversized = true; frame.clear(); }
                else { frame.extend_from_slice(&available[..payload]); }
            }
            input.consume(count);
            if end.is_some() { break; }
        }
        if !saw_bytes { return Ok(()); }
        let response = if oversized { Some(rpc_error(Value::Null, -32600, "Request exceeds 1 MiB")) }
            else { match std::str::from_utf8(&frame) {
                Ok(line) => session.handle_line(line),
                Err(_) => Some(rpc_error(Value::Null, -32700, "Request is not UTF-8")),
            }};
        if let Some(response) = response { write_json(&mut output, &response)?; }
    }
}

fn excluded_name(name: &std::ffi::OsStr) -> bool {
    matches!(name.to_str(), Some(".git" | ".texgo" | "node_modules" | "texgo-pdf"))
}

fn source_extension(path: &Path, include_pdf: bool) -> bool {
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    matches!(extension.as_str(), "tex" | "bib" | "sty" | "cls" | "png" | "jpg" | "jpeg" | "gif" |
        "tif" | "tiff" | "bmp" | "webp" | "svg" | "eps" | "ps") || (include_pdf && extension == "pdf")
}

type Snapshot = BTreeMap<PathBuf, (u64, Option<SystemTime>)>;

fn watch_snapshot(config: &ResolvedConfig) -> Result<Snapshot, String> {
    let mut snapshot = BTreeMap::new();
    // The config is explicitly tracked even when it is inside excluded .texgo.
    if let Ok(metadata) = fs::metadata(&config.config_path) {
        snapshot.insert(config.config_path.clone(), (metadata.len(), metadata.modified().ok()));
    }
    let mut pending = vec![(config.root.clone(), 0usize)];
    let mut visited = 0usize;
    while let Some((directory, depth)) = pending.pop() {
        if depth > 64 { return Err("Watch directory nesting exceeds 64 levels".into()); }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Cannot watch {}: {error}", directory.display())),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            visited += 1;
            if visited > 50_000 { return Err("Watch project exceeds 50000 entries".into()); }
            let path = entry.path();
            if path.starts_with(&config.output_dir) || excluded_name(&entry.file_name()) { continue; }
            let kind = match entry.file_type() { Ok(kind) => kind, Err(_) => continue };
            // Never follow directory links out of the project or into cycles.
            if kind.is_dir() { pending.push((path, depth + 1)); }
            else if kind.is_file() && source_extension(&path, true) {
                if let Ok(metadata) = entry.metadata() {
                    snapshot.insert(path, (metadata.len(), metadata.modified().ok()));
                }
            }
        }
    }
    Ok(snapshot)
}

fn print_build(writer: &mut impl Write, result: &BuildResult) -> io::Result<()> {
    for item in &result.diagnostics {
        writeln!(writer, "{}:{}: {}: {}", item.file.as_deref().unwrap_or("latexmk"), item.line.unwrap_or(1), item.severity, item.message)?;
    }
    writeln!(writer, "[texgo] {} | log: {}", if result.success { "build succeeded" } else { "build failed" }, result.log_path)?;
    if let Some(pdf) = &result.pdf_path { writeln!(writer, "[texgo] PDF: {pdf}")?; }
    writer.flush()
}

fn watch_run(config: &ResolvedConfig, json_output: bool, output: &mut impl Write) -> io::Result<()> {
    if json_output { write_json(output, &json!({"event":"build-start"}))?; }
    else { writeln!(output, "[texgo] build-start")?; output.flush()?; }
    let result = run_pipeline(config);
    match result {
        Ok(result) => {
            if json_output { write_json(output, &json!({"event":"build-result", "result":result}))?; }
            else { print_build(output, &result.build)?; }
        }
        Err(message) => {
            if json_output { write_json(output, &json!({"event":"build-result", "error":{"message":message}}))?; }
            else { writeln!(output, "{}:1: error: {message}", config.main_file.display())?; }
        }
    }
    watch_ready(json_output, output)
}

fn watch_ready(json_output: bool, output: &mut impl Write) -> io::Result<()> {
    if json_output { write_json(output, &json!({"event":"watch-ready"})) }
    else { writeln!(output, "[texgo] watch-ready")?; output.flush() }
}

pub fn watch(config_path: &Path, json_output: bool, mut output: impl Write) -> Result<(), String> {
    let mut config = load_config(config_path)?;
    // Snapshot BEFORE compilation: changes made while latexmk runs remain visible.
    let mut observed = watch_snapshot(&config)?;
    watch_run(&config, json_output, &mut output).map_err(|e| e.to_string())?;
    let mut dirty_since: Option<Instant> = None;
    loop {
        std::thread::sleep(POLL_INTERVAL);
        let current = watch_snapshot(&config)?;
        if current != observed {
            observed = current;
            dirty_since = Some(Instant::now());
        }
        if !dirty_since.is_some_and(|since| since.elapsed() >= DEBOUNCE) { continue; }
        dirty_since = None;
        match load_config(&config.config_path) {
            Ok(updated) => {
                config = updated;
                observed = watch_snapshot(&config)?;
                watch_run(&config, json_output, &mut output).map_err(|e| e.to_string())?;
            }
            Err(message) => {
                if json_output {
                    write_json(&mut output, &json!({"event":"build-start"})).map_err(|e| e.to_string())?;
                    write_json(&mut output, &json!({"event":"build-result","error":{"message":message}})).map_err(|e| e.to_string())?;
                } else {
                    writeln!(output, "[texgo] build-start\n{}:1: error: {message}", config.main_file.display()).map_err(|e| e.to_string())?;
                }
                watch_ready(json_output, &mut output).map_err(|e| e.to_string())?;
            }
        }
    }
}

const USAGE: &str = "texgo-engine run --config PATH [--json]\ntexgo-engine build --config PATH [--json]\ntexgo-engine watch --config PATH [--json]\ntexgo-engine scan --main PATH [--json]\ntexgo-engine rpc";

fn cli_request(args: &[String], output: &mut impl Write) -> Result<bool, String> {
    let action = args.first().map(String::as_str).ok_or(USAGE)?;
    let mut config_path = None;
    let mut main_file = None;
    let mut json_output = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--json" if !json_output => json_output = true,
            "--config" if config_path.is_none() => {
                index += 1;
                let path = args.get(index).filter(|path| !path.starts_with("--")).ok_or("--config requires a path")?;
                config_path = Some(PathBuf::from(path));
            }
            "--main" if main_file.is_none() => {
                index += 1;
                let path = args.get(index).filter(|path| !path.starts_with("--")).ok_or("--main requires a path")?;
                main_file = Some(PathBuf::from(path));
            }
            unknown => return Err(format!("Unexpected argument: {unknown}\n{USAGE}")),
        }
        index += 1;
    }
    match action {
        "scan" if config_path.is_none() => {
            let main = main_file.ok_or("scan requires --main PATH")?;
            let workspace = Workspace::open(&main)?;
            if json_output { write_json(output, workspace.project()).map_err(|e| e.to_string())?; }
            else {
                writeln!(output, "[texgo] {} | {} image(s)", workspace.project().main_file, workspace.project().images.len()).map_err(|e| e.to_string())?;
                for image in &workspace.project().images {
                    writeln!(output, "{}: {}", image.status, image.relative_path).map_err(|e| e.to_string())?;
                }
                for warning in &workspace.project().warnings { writeln!(output, "warning: {warning}").map_err(|e| e.to_string())?; }
            }
            Ok(true)
        }
        "run" | "build" | "watch" if main_file.is_none() => {
            let path = config_path.ok_or("This command requires --config PATH")?;
            if action == "watch" { watch(&path, json_output, output)?; return Ok(true); }
            let config = load_config(&path)?;
            if action == "build" {
                let result = build_document(&config)?;
                if json_output { write_json(output, &result).map_err(|e| e.to_string())?; }
                else { print_build(output, &result).map_err(|e| e.to_string())?; }
                Ok(result.success)
            } else {
                let result = run_pipeline(&config)?;
                if json_output { write_json(output, &result).map_err(|e| e.to_string())?; }
                else { print_build(output, &result.build).map_err(|e| e.to_string())?; }
                Ok(result.build.success)
            }
        }
        _ => Err(format!("Invalid command or flags\n{USAGE}")),
    }
}

pub fn cli(args: &[String]) -> i32 {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if args.len() == 1 && args[0] == "rpc" {
        return match serve_rpc(stdin.lock(), &mut output) {
            Ok(()) => 0,
            Err(error) => { eprintln!("JSON-RPC I/O failed: {error}"); 1 }
        };
    }
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        return if writeln!(output, "{USAGE}").is_ok() { 0 } else { 1 };
    }
    match cli_request(args, &mut output) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(message) => {
            if args.iter().any(|arg| arg == "--json") {
                let _ = write_json(&mut output, &json!({"error":{"message":message}}));
            } else { eprintln!("texgo-engine: {message}"); }
            1
        }
    }
}
