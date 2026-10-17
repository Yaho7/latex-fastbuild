use serde_json::{json, Value};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use texgo_desktop::automation::{load_config, parse_diagnostics, serve_rpc, RpcSession, MAX_REQUEST_BYTES};

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::Builder::new().prefix("texgo automation ").tempdir().unwrap();
    write(dir.path(), "main.tex", "\\documentclass{article}\n\\begin{document}Hello\\end{document}\n");
    write(dir.path(), ".texgo/project.json", &json!({
        "version": 1, "mainFile": "../main.tex",
        "images": {"enabled": true, "replace": true},
        "build": {"engine": "xelatex", "outputDir": "build", "synctex": true}
    }).to_string());
    dir
}

fn request(session: &mut RpcSession, method: &str, params: Value) -> Value {
    session.handle_line(&json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params}).to_string()).unwrap()
}

#[test]
fn config_resolves_main_against_config_and_output_against_main() {
    let dir = fixture();
    let config = load_config(&dir.path().join(".texgo/project.json")).unwrap();
    let root = dir.path().canonicalize().unwrap();
    assert_eq!(config.main_file, root.join("main.tex"));
    assert_eq!(config.output_dir, root.join("build"));
    assert_eq!(config.config.images.quality, "original");
    assert_eq!(config.config.images.dpi, 300);
    assert!(config.config.images.replace);
    let path = dir.path().join(".texgo/project.json");
    write(dir.path(), ".texgo/project.json", &json!({
        "$schema":"../../../docs/project.schema.json", "version":1, "mainFile":"../main.tex"
    }).to_string());
    assert!(load_config(&path).unwrap().config.schema.is_some());
}

#[test]
fn diagnostics_include_tex_error_lines_warnings_and_latexmk_fallbacks() {
    let diagnostics = parse_diagnostics(
        "./chapter.tex:9: Undefined control sequence.\n! Emergency stop.\nl.12 \\bad\nLaTeX Warning: Citation missing on input line 20.\nLatexmk: Errors, so I did not complete making targets\n",
        Path::new("/project/main.tex"),
    );
    assert!(diagnostics.iter().any(|d| d.line == Some(9) && d.file.as_deref().unwrap().ends_with("chapter.tex")));
    assert!(diagnostics.iter().any(|d| d.line == Some(12) && d.severity == "error"));
    assert!(diagnostics.iter().any(|d| d.line == Some(20) && d.severity == "warning"));
    assert!(diagnostics.iter().any(|d| d.message.starts_with("Latexmk:")));
    let windows = parse_diagnostics("C:\\work\\main.tex:7: Bad command\n", Path::new("main.tex"));
    assert_eq!(windows[0].line, Some(7));
}

#[test]
fn config_requires_version_rejects_typos_and_unsafe_settings() {
    let dir = fixture();
    let path = dir.path().join(".texgo/project.json");
    for value in [
        json!({"mainFile":"../main.tex"}),
        json!({"version":2,"mainFile":"../main.tex"}),
        json!({"version":1,"mainFile":"../main.tex","imagse":{}}),
        json!({"version":1,"mainFile":"../main.tex","images":{"replace":"true"}}),
        json!({"version":1,"mainFile":"../main.tex","images":{"quality":"bad"}}),
        json!({"version":1,"mainFile":"../main.tex","images":{"dpi":0}}),
        json!({"version":1,"mainFile":"../main.tex","images":{"maxWidthMm":1}}),
        json!({"version":1,"mainFile":"../main.tex","images":{"jpegQuality":101}}),
        json!({"version":1,"mainFile":"../main.tex","build":{"engine":"sh"}}),
        json!({"version":1,"mainFile":"../main.tex","build":{"outputDir":"../outside"}}),
        json!({"version":1,"mainFile":"../main.tex","build":{"outputDir":"."}}),
        json!({"version":1,"mainFile":"../main.tex","build":{"outputDir":".texgo/build"}}),
    ] {
        fs::write(&path, value.to_string()).unwrap();
        assert!(load_config(&path).is_err(), "accepted {value}");
    }
    fs::write(&path, json!({"version":1,"mainFile":"../main.tex"}).to_string()).unwrap();
    assert!(!load_config(&path).unwrap().config.images.replace);
}

#[cfg(unix)]
#[test]
fn config_rejects_symlink_output_and_source_image_output_collision() {
    let dir = fixture();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("build")).unwrap();
    assert!(load_config(&dir.path().join(".texgo/project.json")).is_err());
    fs::remove_file(dir.path().join("build")).unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{build/source.pdf}");
    write(dir.path(), "build/source.pdf", "%PDF-1.4");
    assert!(load_config(&dir.path().join(".texgo/project.json")).is_err());
}

#[test]
fn rpc_validates_envelopes_ids_params_and_notifications() {
    let mut session = RpcSession::default();
    assert_eq!(session.handle_line("{").unwrap()["error"]["code"], -32700);
    for line in ["[]", "null", "{}", r#"{"jsonrpc":"1.0","id":1,"method":"tools.info"}"#,
        r#"{"jsonrpc":"2.0","id":true,"method":"tools.info"}"#,
        r#"{"jsonrpc":"2.0","id":{},"method":"tools.info"}"#] {
        assert_eq!(session.handle_line(line).unwrap()["error"]["code"], -32600);
    }
    for id in [json!(null), json!("request-1"), json!(42), json!(1.5)] {
        let response = session.handle_line(&json!({"jsonrpc":"2.0","id":id,"method":"tools.info"}).to_string()).unwrap();
        assert_eq!(response["id"], id);
        assert!(response.get("result").is_some());
    }
    assert_eq!(request(&mut session, "no.such.method", json!({}))["error"]["code"], -32601);
    assert_eq!(request(&mut session, "project.open", json!({}))["error"]["code"], -32602);
    assert_eq!(request(&mut session, "tools.info", json!([]))["error"]["code"], -32602);
    assert_eq!(request(&mut session, "tools.info", json!({"unexpected":true}))["error"]["code"], -32602);
    assert_eq!(request(&mut session, "references.preview", json!({}))["error"]["code"], -32000);
    assert!(session.handle_line(r#"{"jsonrpc":"2.0","method":"tools.info"}"#).is_none());
    assert!(session.handle_line(r#"{"jsonrpc":"2.0","method":"no.such.method"}"#).is_none());
    assert!(session.handle_line(r#"{"jsonrpc":"2.0","method":"project.open","params":{}}"#).is_none());
}

#[test]
fn rpc_conversion_preview_confirm_apply_and_persistent_undo() {
    let dir = fixture();
    let source = "\\includegraphics{a.png}\n";
    write(dir.path(), "main.tex", source);
    image::RgbImage::new(4, 2).save(dir.path().join("a.png")).unwrap();
    let mut session = RpcSession::default();
    let opened = request(&mut session, "project.open", json!({"mainFile":dir.path().join("main.tex")}));
    let id = opened["result"]["images"][0]["id"].clone();
    let converted = request(&mut session, "images.convert", json!({"options":{
        "imageIds":[id], "quality":"original", "dpi":300, "maxWidthMm":160, "jpegQuality":90
    }}));
    assert!(converted["result"]["images"][0]["error"].is_null(), "{converted}");
    assert_eq!(request(&mut session, "references.preview", json!({}))["result"]["referenceCount"], 1);
    for params in [json!({}), json!({"confirm":false}), json!({"confirm":"true"})] {
        assert_eq!(request(&mut session, "references.apply", params)["error"]["code"], -32602);
        assert_eq!(fs::read_to_string(dir.path().join("main.tex")).unwrap(), source);
    }
    assert_eq!(request(&mut session, "references.apply", json!({"confirm":true}))["result"]["referencesChanged"], 1);
    assert!(fs::read_to_string(dir.path().join("main.tex")).unwrap().contains("texgo-pdf/"));
    assert!(dir.path().join("a.png").is_file());
    // A new session can undo the persisted transaction, not just an in-memory plan.
    let mut session = RpcSession::default();
    request(&mut session, "project.open", json!({"mainFile":dir.path().join("main.tex")}));
    assert_eq!(request(&mut session, "references.undo", json!({"confirm":false}))["error"]["code"], -32602);
    assert_eq!(request(&mut session, "references.undo", json!({"confirm":true}))["result"]["filesChanged"], 1);
    assert_eq!(fs::read_to_string(dir.path().join("main.tex")).unwrap(), source);
}

#[test]
fn rpc_transport_bounds_requests_recovers_and_flushes_notifications() {
    let mut input = vec![b'x'; MAX_REQUEST_BYTES + 10];
    input.extend_from_slice(b"\n{bad\n{\"jsonrpc\":\"2.0\",\"method\":\"tools.info\"}\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools.info\"}\n");
    let mut output = Vec::new();
    serve_rpc(Cursor::new(input), &mut output).unwrap();
    let responses: Vec<Value> = String::from_utf8(output).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["error"]["code"], -32600);
    assert_eq!(responses[1]["error"]["code"], -32700);
    assert_eq!(responses[2]["id"], 7);
}

#[cfg(all(unix, feature = "automation"))]
mod cli {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Output, Stdio};
    use std::time::{Duration, Instant};

    fn compiler(dir: &Path) -> std::path::PathBuf {
        let bin = dir.join("fake-bin");
        fs::create_dir_all(&bin).unwrap();
        write(&bin, "latexmk", r#"#!/bin/sh
printf '%s\n' "$@" >> compiler-args.txt
printf '%s\n' "$PWD" > compiler-cwd.txt
for arg in "$@"; do
  case "$arg" in -outdir=*) out=${arg#-outdir=} ;; esac
done
if [ -f hold-build ]; then
  touch build-started
  while [ -f hold-build ]; do sleep 0.05; done
fi
if [ -f fail-build ]; then
  printf './main.tex:12: Undefined control sequence.\n! Emergency stop.\nl.12 \\bad\n' >&2
  printf 'Latexmk: Errors, so I did not complete making targets\n'
  exit 7
fi
printf 'LaTeX Warning: Test warning on input line 3.\n'
printf 'native log\n' > "$out/main.log"
printf '%%PDF-1.4\n' > "$out/main.pdf"
exit 0
"#);
        fs::set_permissions(bin.join("latexmk"), fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    fn command(dir: &Path) -> Command {
        let bin = compiler(dir);
        let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()))).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_texgo-engine"));
        // Child-only environment: parallel tests never mutate the parent PATH.
        command.current_dir(dir).env("PATH", path);
        command
    }

    fn invoke(dir: &Path, action: &str) -> Output {
        command(dir).args([action, "--config", ".texgo/project.json", "--json"]).output().unwrap()
    }

    #[test]
    fn build_uses_argv_cwd_engine_and_separate_logs_without_editing() {
        let dir = fixture();
        let before = fs::read(dir.path().join("main.tex")).unwrap();
        let output = invoke(dir.path(), "build");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["exitCode"], 0);
        assert!(Path::new(result["pdfPath"].as_str().unwrap()).is_file());
        let log = Path::new(result["logPath"].as_str().unwrap());
        assert_eq!(log.file_name().unwrap(), "main.texgo-output.log");
        assert!(fs::read_to_string(log).unwrap().contains("Test warning"));
        assert_eq!(fs::read_to_string(dir.path().join("build/main.log")).unwrap(), "native log\n");
        let args = fs::read_to_string(dir.path().join("compiler-args.txt")).unwrap();
        for expected in ["-xelatex", "-synctex=1", "-file-line-error", "-interaction=nonstopmode", "-halt-on-error", "main.tex"] {
            assert!(args.lines().any(|line| line == expected), "missing {expected}: {args}");
        }
        assert!(args.lines().any(|line| line.starts_with("-outdir=")));
        assert_eq!(fs::read_to_string(dir.path().join("compiler-cwd.txt")).unwrap().trim(), dir.path().canonicalize().unwrap().to_str().unwrap());
        assert_eq!(fs::read(dir.path().join("main.tex")).unwrap(), before);
    }

    #[test]
    fn failed_compiles_keep_json_details_full_logs_and_nonzero_exit() {
        let dir = fixture();
        write(dir.path(), "fail-build", "");
        let output = invoke(dir.path(), "build");
        assert!(!output.status.success());
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["success"], false);
        assert_eq!(result["exitCode"], 7);
        assert!(result["pdfPath"].is_null());
        assert!(result["diagnostics"].as_array().unwrap().iter().any(|d| d["line"] == 12 && d["severity"] == "error"));
        let log = fs::read_to_string(result["logPath"].as_str().unwrap()).unwrap();
        assert!(log.contains("Undefined control sequence"));
        assert!(log.contains("Latexmk: Errors"));
        let pipeline = invoke(dir.path(), "run");
        assert!(!pipeline.status.success());
        let result: Value = serde_json::from_slice(&pipeline.stdout).unwrap();
        assert_eq!(result["build"]["success"], false);
        assert_eq!(result["build"]["exitCode"], 7);
    }

    #[test]
    fn compiler_engine_flags_and_synctex_off_are_selected_without_shell() {
        for (engine, flag) in [("pdflatex", "-pdf"), ("lualatex", "-lualatex")] {
            let dir = fixture();
            write(dir.path(), ".texgo/project.json", &json!({
                "version":1, "mainFile":"../main.tex",
                "build":{"engine":engine,"outputDir":"build with spaces","synctex":false}
            }).to_string());
            let result = invoke(dir.path(), "build");
            assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stdout));
            let args = fs::read_to_string(dir.path().join("compiler-args.txt")).unwrap();
            assert!(args.lines().any(|arg| arg == flag));
            assert!(args.lines().any(|arg| arg == "-synctex=0"));
            assert!(args.lines().any(|arg| arg.starts_with("-outdir=") && arg.ends_with("build with spaces")));
        }
    }

    #[test]
    fn project_write_lock_rejects_compile_before_spawning() {
        let dir = fixture();
        let lock = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .open(dir.path().join(".texgo/write.lock")).unwrap();
        fs2::FileExt::lock_exclusive(&lock).unwrap();
        let result = invoke(dir.path(), "build");
        assert!(!result.status.success());
        assert!(serde_json::from_slice::<Value>(&result.stdout).unwrap().get("error").is_some());
        assert!(!dir.path().join("compiler-args.txt").exists());
    }

    #[test]
    fn missing_compiler_is_logged_and_returned_as_failed_build() {
        let dir = fixture();
        let output = Command::new(env!("CARGO_BIN_EXE_texgo-engine"))
            .current_dir(dir.path()).env("PATH", "")
            .args(["build", "--config", ".texgo/project.json", "--json"]).output().unwrap();
        assert!(!output.status.success());
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["success"], false);
        assert!(result["exitCode"].is_null());
        assert!(fs::read_to_string(result["logPath"].as_str().unwrap()).unwrap().contains("latexmk"));
    }

    #[test]
    fn pipeline_converts_once_preserves_sources_and_is_idempotent() {
        let dir = fixture();
        write(dir.path(), "main.tex", "\\includegraphics{a.png}\n");
        image::RgbImage::new(4, 2).save(dir.path().join("a.png")).unwrap();
        let first = invoke(dir.path(), "run");
        assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stdout));
        let result: Value = serde_json::from_slice(&first.stdout).unwrap();
        assert_eq!(result["replacement"]["referencesChanged"], 1);
        let replaced = fs::read(dir.path().join("main.tex")).unwrap();
        let undo = fs::read(dir.path().join(".texgo/undo.json")).unwrap();
        let second = invoke(dir.path(), "run");
        assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stdout));
        let result: Value = serde_json::from_slice(&second.stdout).unwrap();
        assert!(result["replacement"].is_null());
        assert_eq!(fs::read(dir.path().join("main.tex")).unwrap(), replaced);
        assert_eq!(fs::read(dir.path().join(".texgo/undo.json")).unwrap(), undo);
        assert!(dir.path().join("a.png").is_file());
    }

    #[test]
    fn pipeline_rejects_unresolved_and_conversion_errors_before_compile() {
        for reference in ["missing.png", "broken.png", "\\macro/file"] {
            let dir = fixture();
            write(dir.path(), "main.tex", &format!("\\includegraphics{{{reference}}}"));
            write(dir.path(), "broken.png", "not an image");
            let before = fs::read(dir.path().join("main.tex")).unwrap();
            let output = invoke(dir.path(), "run");
            assert!(!output.status.success());
            assert!(serde_json::from_slice::<Value>(&output.stdout).unwrap().get("error").is_some());
            assert!(!dir.path().join("compiler-args.txt").exists());
            assert_eq!(fs::read(dir.path().join("main.tex")).unwrap(), before);
        }
    }

    #[test]
    fn omitted_replace_converts_but_never_edits_references() {
        let dir = fixture();
        write(dir.path(), "main.tex", "\\includegraphics{a.png}");
        image::RgbImage::new(4, 2).save(dir.path().join("a.png")).unwrap();
        write(dir.path(), ".texgo/project.json", &json!({"version":1,"mainFile":"../main.tex"}).to_string());
        let output = invoke(dir.path(), "run");
        assert!(output.status.success());
        assert_eq!(fs::read_to_string(dir.path().join("main.tex")).unwrap(), "\\includegraphics{a.png}");
        assert!(!dir.path().join(".texgo/undo.json").exists());
    }

    fn wait_for(mut condition: impl FnMut() -> bool) {
        let start = Instant::now();
        while !condition() {
            assert!(start.elapsed() < Duration::from_secs(15), "watch condition timed out");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn runs(path: &Path) -> usize {
        fs::read_to_string(path.join("compiler-args.txt")).unwrap_or_default().lines().filter(|line| *line == "main.tex").count()
    }

    fn events(path: &Path) -> Vec<Value> {
        fs::read_to_string(path.join("watch.ndjson")).unwrap_or_default().lines()
            .filter_map(|line| serde_json::from_str(line).ok()).collect()
    }

    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
    }

    #[test]
    fn watch_keeps_edits_during_compile_detects_add_delete_and_ignores_outputs() {
        let dir = fixture();
        write(dir.path(), "hold-build", "");
        let stream = fs::File::create(dir.path().join("watch.ndjson")).unwrap();
        let child = command(dir.path()).args(["watch", "--config", ".texgo/project.json", "--json"])
            .stdout(Stdio::from(stream)).stderr(Stdio::null()).spawn().unwrap();
        let mut child = ChildGuard(child);
        wait_for(|| dir.path().join("build-started").exists());
        write(dir.path(), "chapter.tex", "edit during compilation");
        fs::remove_file(dir.path().join("hold-build")).unwrap();
        wait_for(|| runs(dir.path()) >= 2);
        std::thread::sleep(Duration::from_millis(1100));
        let stable = runs(dir.path());
        for name in ["build/ignored.pdf", ".git/ignored.tex", ".texgo/ignored.tex", "texgo-pdf/ignored.pdf", "node_modules/ignored.tex"] {
            write(dir.path(), name, "ignored");
        }
        std::thread::sleep(Duration::from_millis(1100));
        assert_eq!(runs(dir.path()), stable);
        write(dir.path(), "fail-build", "");
        write(dir.path(), "refs.bib", "new bibliography");
        wait_for(|| events(dir.path()).iter().any(|v| v["event"] == "build-result" && v["result"]["build"]["success"] == false));
        let failed = runs(dir.path());
        fs::remove_file(dir.path().join("fail-build")).unwrap();
        fs::remove_file(dir.path().join("refs.bib")).unwrap();
        wait_for(|| runs(dir.path()) > failed);
        wait_for(|| events(dir.path()).iter().filter(|v| v["event"] == "watch-ready").count() > failed);
        // The config is watched even though other .texgo files are excluded.
        write(dir.path(), ".texgo/project.json", &json!({
            "version":1, "mainFile":"../main.tex", "build":{"engine":"lualatex"}
        }).to_string());
        wait_for(|| fs::read_to_string(dir.path().join("compiler-args.txt")).unwrap_or_default()
            .lines().any(|arg| arg == "-lualatex"));
        assert!(child.0.try_wait().unwrap().is_none());
        let events = events(dir.path());
        assert!(events.iter().any(|v| v["event"] == "watch-ready"));
        assert!(events.iter().any(|v| v["event"] == "build-result" && v["result"]["build"]["success"] == false));
    }
}
