use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use texgo_desktop::{ConversionOptions, Workspace};

fn write(root: &Path, path: &str, text: &str) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
}

fn png(root: &Path, path: &str) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(32, 16, image::Rgba([15, 120, 100, 128]))
        .save(target)
        .unwrap();
}

fn options(workspace: &Workspace) -> ConversionOptions {
    ConversionOptions {
        image_ids: workspace.project().images.iter().map(|i| i.id.clone()).collect(),
        quality: "original".into(),
        dpi: 300,
        max_width_mm: 160.0,
        jpeg_quality: 90,
    }
}

#[test]
fn recursively_resolves_graphicspath_and_unicode_references() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\graphicspath{{插图/}}\n\\input{chapters/body}\n");
    write(dir.path(), "chapters/body.tex", "\\includegraphics[width=.8\\linewidth]{实验.png}\n\\includegraphics{实验.png}");
    png(dir.path(), "插图/实验.png");
    let ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    assert_eq!(ws.project().tex_files.len(), 2);
    assert_eq!(ws.project().images.len(), 1);
    let asset = &ws.project().images[0];
    assert_eq!(asset.status, "ready");
    assert_eq!(asset.references.len(), 2);
    assert_eq!(asset.references[0].line, 1);
    assert_eq!(asset.width, Some(32));
}

#[test]
fn skips_comments_verbatim_and_cycles_and_reports_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "% \\includegraphics{comment.png}\n\\begin{verbatim}\n\\includegraphics{code.png}\n\\end{verbatim}\n\\verb|\\includegraphics{inline.png}|\n\\input{main}\n\\includegraphics{missing.png}");
    let ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    assert_eq!(ws.project().images.len(), 1);
    assert_eq!(ws.project().images[0].status, "missing");
    assert!(!ws.project().warnings.is_empty());
}

#[test]
fn handles_starred_multiline_commands_and_nested_options() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics*\n[width={.5\\textwidth},trim={0 0 0 0}]%comment\n{figures/a.png}");
    png(dir.path(), "figures/a.png");
    let ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    assert_eq!(ws.project().images.len(), 1);
    assert_eq!(ws.project().images[0].status, "ready");
}

#[test]
fn ambiguous_extensionless_and_macro_paths_are_not_guessed() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{figures/a}\n\\includegraphics{\\myfigure/path}");
    png(dir.path(), "figures/a.png");
    image::RgbImage::new(2, 2).save(dir.path().join("figures/a.jpg")).unwrap();
    let ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    assert_eq!(ws.project().images[0].status, "ambiguous");
    assert_eq!(ws.project().images[1].status, "unsupported");
}

#[test]
fn creates_readable_pdf_and_content_cache_with_transparency() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{a.png}");
    png(dir.path(), "a.png");
    let mut ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    let opts = options(&ws);
    let cancelled = AtomicBool::new(false);
    let first = ws.convert(&opts, &cancelled, |_| {}).unwrap();
    assert_eq!(first.images.len(), 1);
    assert!(first.images[0].error.is_none());
    let pdf = first.images[0].pdf_path.as_ref().unwrap();
    let doc = lopdf::Document::load(pdf).unwrap();
    assert_eq!(doc.get_pages().len(), 1);
    assert!(doc.objects.values().any(|obj| obj.as_stream().is_ok_and(|s| s.dict.has(b"SMask"))));
    let second = ws.convert(&opts, &cancelled, |_| {}).unwrap();
    assert!(second.images[0].cached);
    let mut changed = opts;
    changed.quality = "compact".into();
    let third = ws.convert(&changed, &cancelled, |_| {}).unwrap();
    assert_ne!(third.images[0].pdf_path.as_ref().unwrap(), pdf);
}

#[test]
fn converts_svg_without_external_tools() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{plot.svg}");
    write(dir.path(), "plot.svg", r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect width="100" height="50" fill="teal"/></svg>"#);
    let mut ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    let result = ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(result.images[0].error.is_none(), "{:?}", result.images[0]);
    assert_eq!(lopdf::Document::load(result.images[0].pdf_path.as_ref().unwrap()).unwrap().get_pages().len(), 1);
}

#[test]
fn cancellation_prevents_conversion_and_invalid_quality_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{a.png}");
    png(dir.path(), "a.png");
    let mut ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    let mut opts = options(&ws);
    let result = ws.convert(&opts, &AtomicBool::new(true), |_| {}).unwrap();
    assert!(result.cancelled);
    assert!(result.images.is_empty());
    opts.quality = "unknown".into();
    assert!(ws.convert(&opts, &AtomicBool::new(false), |_| {}).is_err());
}

#[test]
fn replaces_only_path_spans_and_persists_undo_across_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let source = "% comment\r\n\\includegraphics[width=.8\\linewidth]{a.png}\r\nText a.png\r\n";
    write(dir.path(), "main.tex", source);
    png(dir.path(), "a.png");
    let main = dir.path().join("main.tex");
    let mut ws = Workspace::open(&main).unwrap();
    ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    let plan = ws.plan().unwrap();
    assert_eq!(plan.reference_count, 1);
    let result = ws.apply().unwrap();
    assert_eq!(result.files_changed, 1);
    let updated = fs::read_to_string(&main).unwrap();
    assert!(updated.contains("Text a.png\r\n"));
    assert!(updated.contains("[width=.8\\linewidth]{"));
    assert!(updated.contains("texgo-pdf/"));
    assert!(dir.path().join("a.png").is_file());
    let mut reopened = Workspace::open(&main).unwrap();
    assert!(reopened.project().undo_available);
    reopened.undo().unwrap();
    assert_eq!(fs::read_to_string(main).unwrap(), source);
}

#[test]
fn stale_plan_and_undo_never_overwrite_editor_changes() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{a.png}");
    png(dir.path(), "a.png");
    let main = dir.path().join("main.tex");
    let mut ws = Workspace::open(&main).unwrap();
    ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    fs::write(&main, "editor changed this").unwrap();
    assert!(ws.apply().is_err());
    assert_eq!(fs::read_to_string(&main).unwrap(), "editor changed this");
    write(dir.path(), "main.tex", "\\includegraphics{a.png}");
    let mut ws = Workspace::open(&main).unwrap();
    ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    ws.apply().unwrap();
    fs::write(&main, "editor changed after replacement").unwrap();
    assert!(ws.undo().is_err());
    assert_eq!(fs::read_to_string(main).unwrap(), "editor changed after replacement");
}

#[test]
fn rejects_references_outside_project_and_existing_pdf_is_not_converted() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "project/main.tex", "\\input{../outside}\n\\includegraphics{already.pdf}");
    write(dir.path(), "outside.tex", "\\includegraphics{outside.png}");
    write(dir.path(), "project/already.pdf", "%PDF-1.4\n");
    let ws = Workspace::open(&dir.path().join("project/main.tex")).unwrap();
    assert_eq!(ws.project().tex_files.len(), 1);
    assert_eq!(ws.project().images.len(), 1);
    assert_eq!(ws.project().images[0].status, "pdf");
    assert!(!ws.project().warnings.is_empty());
}

#[test]
fn corrupt_cache_is_rebuilt_and_modified_pdf_blocks_replacement() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{a.png}");
    png(dir.path(), "a.png");
    let mut ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    let opts = options(&ws);
    let first = ws.convert(&opts, &AtomicBool::new(false), |_| {}).unwrap();
    let pdf = first.images[0].pdf_path.as_ref().unwrap();
    fs::write(pdf, "broken").unwrap();
    assert!(ws.plan().is_err());
    let rebuilt = ws.convert(&opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(!rebuilt.images[0].cached);
    assert!(rebuilt.images[0].error.is_none());
    assert_eq!(ws.plan().unwrap().reference_count, 1);
}

#[test]
fn multi_file_replacement_preflight_rejects_all_when_one_file_changed() {
    let dir = tempfile::tempdir().unwrap();
    let main_source = "\\includegraphics{a.png}\n\\input{chapter}";
    write(dir.path(), "main.tex", main_source);
    write(dir.path(), "chapter.tex", "\\includegraphics{a.png}");
    png(dir.path(), "a.png");
    let mut ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    write(dir.path(), "chapter.tex", "changed");
    assert!(ws.apply().is_err());
    assert_eq!(fs::read_to_string(dir.path().join("main.tex")).unwrap(), main_source);
}

#[cfg(unix)]
#[test]
fn conversion_refuses_symlinked_output_directory() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{a.png}");
    png(dir.path(), "a.png");
    std::os::unix::fs::symlink(outside.path(), dir.path().join("texgo-pdf")).unwrap();
    let mut ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    let result = ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(result.images[0].error.as_ref().unwrap().contains("符号链接"));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn unicode_verbatim_delimiters_do_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\verbéé\n\\includegraphics{a.png}");
    png(dir.path(), "a.png");
    let ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    assert_eq!(ws.project().images.len(), 1);
}

#[test]
fn contextual_graphics_paths_block_automatic_replacement() {
    let dir = tempfile::tempdir().unwrap();
    png(dir.path(), "base/a.png");
    png(dir.path(), "alternate/a.png");
    write(dir.path(), "shared.tex", "\\includegraphics{a.png}");
    for source in [
        "\\graphicspath{{base/}}\\input{shared}\\graphicspath{{alternate/}}\\input{shared}",
        "\\graphicspath{{base/}}{\\graphicspath{{alternate/}}\\includegraphics{a.png}}\\includegraphics{a.png}",
        "\\graphicspath{{base/}}\\iffalse\\graphicspath{{alternate/}}\\fi\\includegraphics{a.png}",
    ] {
        write(dir.path(), "main.tex", source);
        let ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
        assert!(ws.project().images.iter().all(|image| image.status != "ready"));
        assert!(!ws.project().warnings.is_empty());
    }
}

#[test]
fn svg_resources_are_bounded_and_participate_in_cache_and_staleness_checks() {
    let dir = tempfile::tempdir().unwrap();
    png(dir.path(), "photo.png");
    write(dir.path(), "project/main.tex", "\\includegraphics{plot.svg}");
    write(dir.path(), "project/plot.svg", r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><image href="../photo.png" width="32" height="16"/></svg>"#);
    let main = dir.path().join("project/main.tex");
    let mut ws = Workspace::open(&main).unwrap();
    let result = ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(result.images[0].error.is_some());
    png(dir.path(), "project/photo.png");
    write(dir.path(), "project/plot.svg", r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><image href="photo.png" width="32" height="16"/></svg>"#);
    let mut ws = Workspace::open(&main).unwrap();
    let first = ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(first.images[0].error.is_none());
    image::RgbaImage::from_pixel(32, 16, image::Rgba([200, 5, 5, 255])).save(dir.path().join("project/photo.png")).unwrap();
    assert!(ws.plan().is_err());
    let second = ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(second.images[0].error.is_none());
    assert_ne!(first.images[0].pdf_path, second.images[0].pdf_path);
}

#[test]
fn png_density_preserves_natural_page_dimensions_across_quality_modes() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{a.png}");
    let file = fs::File::create(dir.path().join("a.png")).unwrap();
    let mut encoder = png::Encoder::new(file, 200, 100);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_pixel_dims(Some(png::PixelDimensions { xppu: 11811, yppu: 11811, unit: png::Unit::Meter }));
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&vec![0; 200 * 100 * 3]).unwrap();
    writer.finish().unwrap();
    let mut ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    let mut opts = options(&ws);
    for quality in ["original", "compact"] {
        opts.quality = quality.into();
        let result = ws.convert(&opts, &AtomicBool::new(false), |_| {}).unwrap();
        let doc = lopdf::Document::load(result.images[0].pdf_path.as_ref().unwrap()).unwrap();
        let page = *doc.get_pages().values().next().unwrap();
        let bounds = doc.get_object(page).unwrap().as_dict().unwrap().get(b"MediaBox").unwrap().as_array().unwrap();
        let width = match bounds[2] { lopdf::Object::Real(value) => value, lopdf::Object::Integer(value) => value as f32, _ => panic!("invalid page width") };
        assert!((width - 48.0).abs() < 0.01, "unexpected width: {width}");
    }
}

#[test]
fn project_write_lock_prevents_concurrent_apply_without_clobbering_journal() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\includegraphics{a.png}");
    png(dir.path(), "a.png");
    let mut ws = Workspace::open(&dir.path().join("main.tex")).unwrap();
    ws.convert(&options(&ws), &AtomicBool::new(false), |_| {}).unwrap();
    fs::create_dir_all(dir.path().join(".texgo")).unwrap();
    let lock = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.path().join(".texgo/write.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(ws.apply().unwrap_err().contains("另一个"));
    assert!(!dir.path().join(".texgo/undo.json").exists());
    drop(lock);
    ws.apply().unwrap();
    assert!(dir.path().join(".texgo/undo.json").exists());
}
