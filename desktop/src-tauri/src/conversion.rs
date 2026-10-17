use crate::models::ConversionOptions;
use crate::workspace::{digest, save_atomic};
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, DynamicImage, GenericImageView};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use std::fs;
use std::path::{Path, PathBuf};

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

pub(crate) fn convert(source: &Path, options: &ConversionOptions) -> Result<(PathBuf, bool), String> {
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 * 1024 { return Err("单张图片超过 256 MB，请先缩小文件".into()); }
    let settings = format!("texgo-pdf-v1:{}:{}:{}:{}", options.quality, options.dpi, options.max_width_mm, options.jpeg_quality);
    let mut content = bytes.clone();
    content.extend_from_slice(settings.as_bytes());
    let hash = digest(&content);
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
        return Ok((output, true));
    }
    let pdf = if source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")) {
        let mut settings = svg2pdf::usvg::Options::default();
        settings.resources_dir = source.parent().map(Path::to_path_buf);
        settings.fontdb_mut().load_system_fonts();
        let tree = svg2pdf::usvg::Tree::from_data(&bytes, &settings).map_err(|e| format!("SVG 解析失败：{e}"))?;
        svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions::default())
            .map_err(|e| format!("SVG 转换失败：{e}"))?
    } else {
        let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(40_000);
        limits.max_image_height = Some(40_000);
        limits.max_alloc = Some(512 * 1024 * 1024);
        reader.limits(limits);
        let mut image = reader.decode().map_err(|e| format!("图片解码失败：{e}"))?;
        if options.quality != "original" {
            let dpi = match options.quality.as_str() { "high" => 300, "compact" => 150, _ => options.dpi };
            let max_width = (options.max_width_mm / 25.4 * f64::from(dpi)).round().max(1.0) as u32;
            if image.width() > max_width {
                let height = (f64::from(image.height()) * f64::from(max_width) / f64::from(image.width())).round().max(1.0) as u32;
                image = image.resize_exact(max_width, height, FilterType::Lanczos3);
            }
        }
        raster_pdf(&image, options)?
    };
    let document = Document::load_mem(&pdf).map_err(|e| format!("生成的 PDF 无法读取：{e}"))?;
    if document.get_pages().len() != 1 { return Err("转换结果不是单页图片 PDF".into()); }
    save_atomic(&output, &pdf)?;
    Ok((output, false))
}

fn raster_pdf(image: &DynamicImage, options: &ConversionOptions) -> Result<Vec<u8>, String> {
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
    let page_width = (options.max_width_mm / 25.4 * 72.0) as f32;
    let page_height = page_width * height as f32 / width as f32;
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
