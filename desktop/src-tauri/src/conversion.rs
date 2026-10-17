use crate::models::ConversionOptions;
use crate::workspace::{digest, save_atomic};
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, DynamicImage, GenericImageView};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(crate) fn validate(options: &ConversionOptions) -> Result<(), String> {
    if !matches!(options.quality.as_str(), "original" | "high" | "compact" | "custom") {
        return Err("不支持的质量选项".into());
    }
    if !(72..=1200).contains(&options.dpi) || !options.max_width_mm.is_finite()
        || !(10.0..=1000.0).contains(&options.max_width_mm) || !(1..=100).contains(&options.jpeg_quality) {
        return Err("参数超出范围：PPI 72–1200，宽度 10–1000 mm，JPEG 质量 1–100".into());
    }
    Ok(())
}

pub(crate) fn convert(source: &Path, root: &Path, options: &ConversionOptions) -> Result<(PathBuf, bool, Vec<(PathBuf, String)>), String> {
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 * 1024 { return Err("单张图片超过 256 MB，请先缩小文件".into()); }
    let is_svg = source.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("svg"));
    let (vector_pdf, dependencies) = if is_svg {
        let (pdf, dependencies) = svg_pdf(&bytes, source, root)?;
        (Some(pdf), dependencies)
    } else { (None, Vec::new()) };
    let source_identity = source.strip_prefix(root).map_err(|_| "原图片位于项目之外")?.to_string_lossy().replace('\\', "/");
    let dependency_keys: Vec<_> = dependencies.iter().map(|(path, hash)| {
        (path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"), hash)
    }).collect();
    let effective = if is_svg { ("vector", 0, 0.0, 0) } else {
        match options.quality.as_str() {
            "original" => ("original", 0, 0.0, 0),
            "high" => ("high", 300, options.max_width_mm, 92),
            "compact" => ("compact", 150, options.max_width_mm, 75),
            _ => ("custom", options.dpi, options.max_width_mm, options.jpeg_quality),
        }
    };
    // Ordered fields make GUI and headless cache identities independent of JSON map features.
    let identity = ("texgo-pdf-v3", env!("CARGO_PKG_VERSION"), source_identity, digest(&bytes),
        effective, dependency_keys, vector_pdf.as_ref().map(|pdf| digest(pdf)));
    let hash = digest(&serde_json::to_vec(&identity).map_err(|e| e.to_string())?);
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let stem: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(60).collect();
    let output_dir = source.parent().ok_or("图片目录不存在")?.join("texgo-pdf");
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    if output_dir.canonicalize().map_err(|e| e.to_string())? != output_dir {
        return Err("输出目录不能是符号链接，请移除 texgo-pdf 链接后重试".into());
    }
    let output = output_dir.join(format!("{stem}-{}.pdf", &hash[..20]));
    if fs::symlink_metadata(&output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("缓存文件不能是符号链接".into());
    }
    if output.is_file() && Document::load(&output).is_ok_and(|doc| !doc.get_pages().is_empty()) {
        return Ok((output, true, dependencies));
    }
    let pdf = if let Some(pdf) = vector_pdf {
        pdf
    } else {
        let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(40_000);
        limits.max_image_height = Some(40_000);
        limits.max_alloc = Some(512 * 1024 * 1024);
        reader.limits(limits);
        let mut image = reader.decode().map_err(|e| format!("图片解码失败：{e}"))?;
        let (dpi_x, dpi_y) = density(&bytes);
        let physical = (image.width() as f32 / dpi_x * 72.0, image.height() as f32 / dpi_y * 72.0);
        if options.quality != "original" {
            let dpi = match options.quality.as_str() { "high" => 300, "compact" => 150, _ => options.dpi };
            let max_width = (options.max_width_mm / 25.4 * f64::from(dpi)).round().max(1.0) as u32;
            if image.width() > max_width {
                let height = (f64::from(image.height()) * f64::from(max_width) / f64::from(image.width())).round().max(1.0) as u32;
                image = image.resize_exact(max_width, height, FilterType::Lanczos3);
            }
        }
        raster_pdf(&image, options, physical)?
    };
    let document = Document::load_mem(&pdf).map_err(|e| format!("生成的 PDF 无法读取：{e}"))?;
    if document.get_pages().len() != 1 { return Err("转换结果不是单页图片 PDF".into()); }
    save_atomic(&output, &pdf)?;
    Ok((output, false, dependencies))
}

fn svg_pdf(bytes: &[u8], source: &Path, root: &Path) -> Result<(Vec<u8>, Vec<(PathBuf, String)>), String> {
    let mut settings = svg2pdf::usvg::Options::default();
    settings.resources_dir = source.parent().map(Path::to_path_buf);
    settings.fontdb_mut().load_system_fonts();
    let dependencies = Arc::new(Mutex::new(std::collections::BTreeMap::new()));
    let rejected = Arc::new(Mutex::new(Vec::new()));
    let collected = dependencies.clone();
    let denied = rejected.clone();
    let root = root.to_path_buf();
    let resolver = svg2pdf::usvg::ImageHrefResolver::default_data_resolver();
    settings.image_href_resolver.resolve_string = Box::new(move |href, options| {
        let path = options.get_abs_path(Path::new(href));
        let resolved = path.canonicalize().ok().filter(|p| p.starts_with(&root));
        if let Some(path) = resolved {
            if fs::metadata(&path).is_ok_and(|m| m.len() <= 64 * 1024 * 1024) {
                if let Ok(data) = fs::read(&path) {
                    let mime = match image::guess_format(&data) {
                        Ok(image::ImageFormat::Png) => "image/png",
                        Ok(image::ImageFormat::Jpeg) => "image/jpeg",
                        Ok(image::ImageFormat::Gif) => "image/gif",
                        Ok(image::ImageFormat::WebP) => "image/webp",
                        _ if path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("svg") || ext.eq_ignore_ascii_case("svgz")) => "image/svg+xml",
                        _ => { denied.lock().unwrap().push(href.to_string()); return None; },
                    };
                    collected.lock().unwrap().insert(path.clone(), digest(&data));
                    let decoded = resolver(mime, Arc::new(data), options);
                    if decoded.is_none() { denied.lock().unwrap().push(href.to_string()); }
                    return decoded;
                }
            }
        }
        denied.lock().unwrap().push(href.to_string());
        None
    });
    let tree = svg2pdf::usvg::Tree::from_data(bytes, &settings).map_err(|e| format!("SVG 解析失败：{e}"))?;
    if !rejected.lock().unwrap().is_empty() { return Err("SVG 包含项目外、缺失或过大的图片资源，请嵌入资源后重试".into()); }
    let pdf = svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions::default()).map_err(|e| format!("SVG 转换失败：{e}"))?;
    let dependencies = dependencies.lock().unwrap().iter().map(|(path, hash)| (path.clone(), hash.clone())).collect();
    Ok((pdf, dependencies))
}

// Preserve the image's natural TeX size while changing its encoding or sampling.
fn density(bytes: &[u8]) -> (f32, f32) {
    if bytes.starts_with(b"\x89PNG") {
        if let Ok(reader) = png::Decoder::new(std::io::Cursor::new(bytes)).read_info() {
            if let Some(pixels) = reader.info().pixel_dims {
                if pixels.unit == png::Unit::Meter && pixels.xppu > 0 && pixels.yppu > 0 {
                    return (pixels.xppu as f32 * 0.0254, pixels.yppu as f32 * 0.0254);
                }
            }
        }
    }
    if let Ok(jpeg) = img_parts::jpeg::Jpeg::from_bytes(bytes.to_vec().into()) {
        for segment in jpeg.segments() {
            let data = segment.contents();
            if segment.marker() == 0xe0 && data.starts_with(b"JFIF\0") && data.len() >= 12 {
                let factor = match data[7] { 1 => 1.0, 2 => 2.54, _ => continue };
                let x = u16::from_be_bytes([data[8], data[9]]) as f32 * factor;
                let y = u16::from_be_bytes([data[10], data[11]]) as f32 * factor;
                if x > 0.0 && y > 0.0 { return (x, y); }
            }
        }
    }
    if let Ok(metadata) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) {
        let number = |tag| metadata.get_field(tag, exif::In::PRIMARY).and_then(|field| match &field.value {
            exif::Value::Rational(values) => values.first().map(|n| n.to_f64() as f32), _ => None,
        });
        let unit = metadata.get_field(exif::Tag::ResolutionUnit, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0));
        if let (Some(x), Some(y), Some(unit)) = (number(exif::Tag::XResolution), number(exif::Tag::YResolution), unit) {
            if x.is_finite() && y.is_finite() && x > 0.0 && y > 0.0 && matches!(unit, 2 | 3) {
                let factor = if unit == 3 { 2.54 } else { 1.0 };
                return (x * factor, y * factor);
            }
        }
    }
    (72.0, 72.0)
}

fn raster_pdf(image: &DynamicImage, options: &ConversionOptions, physical: (f32, f32)) -> Result<Vec<u8>, String> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 { return Err("图片尺寸无效".into()); }
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let mut dictionary = dictionary! {
        "Type" => "XObject", "Subtype" => "Image", "Width" => width,
        "Height" => height, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
    };
    let rgba = image.to_rgba8();
    let transparent = rgba.pixels().any(|pixel| pixel[3] != 255);
    if transparent {
        let mask: Vec<u8> = rgba.pixels().map(|pixel| pixel[3]).collect();
        let mask_id = doc.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => width,
            "Height" => height, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
        }, mask));
        dictionary.set("SMask", mask_id);
    }
    let rgb = image.to_rgb8();
    let content = if options.quality != "original" && !transparent {
        let quality = match options.quality.as_str() { "high" => 92, "compact" => 75, _ => options.jpeg_quality };
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, quality).encode_image(&DynamicImage::ImageRgb8(rgb)).map_err(|e| e.to_string())?;
        dictionary.set("Filter", "DCTDecode");
        jpeg
    } else { rgb.into_raw() };
    let image_id = doc.add_object(Stream::new(dictionary, content));
    let (page_width, page_height) = physical;
    let operations = Content { operations: vec![
        Operation::new("q", vec![]),
        Operation::new("cm", vec![Object::Real(page_width), 0.into(), 0.into(), Object::Real(page_height), 0.into(), 0.into()]),
        Operation::new("Do", vec![Object::Name(b"Image".to_vec())]),
        Operation::new("Q", vec![]),
    ] };
    let contents = doc.add_object(Stream::new(dictionary! {}, operations.encode().map_err(|e| e.to_string())?));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages, "Contents" => contents,
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(page_width), Object::Real(page_height)],
        "Resources" => dictionary! { "XObject" => dictionary! { "Image" => image_id } },
    });
    doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    doc.compress();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
