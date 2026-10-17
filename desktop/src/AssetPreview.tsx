import { lazy, Suspense, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileImage, LoaderCircle, TriangleAlert } from "lucide-react";
import type { AssetPreviewData, ImageAsset, ImageResult } from "./types";
import { errorMessage, formatBytes } from "./utils";

const PdfPreview = lazy(() => import("./PdfPreview").catch(() => ({ default: () => <p className="preview-error" role="alert">PDF 预览组件加载失败，请重新打开项目</p> })));

interface Props {
  asset: ImageAsset | null;
  result?: ImageResult;
  revision: number;
}

export default function AssetPreview({ asset, result, revision }: Props) {
  const [converted, setConverted] = useState(false);
  const [preview, setPreview] = useState<AssetPreviewData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const hasConverted = Boolean(result?.pdfPath && !result.error);
  const showConverted = converted && hasConverted;
  const unavailable = !asset || !asset.path || ["missing", "ambiguous", "unsupported"].includes(asset.status);
  const assetId = asset?.id;
  const convertedPath = result?.pdfPath;

  useEffect(() => {
    let disposed = false;
    setPreview(null);
    setError(null);
    if (unavailable || !assetId) { setLoading(false); return; }
    setLoading(true);
    // A late IPC response must never overwrite a newer image or preview mode.
    void invoke<AssetPreviewData>("asset_preview", { imageId: assetId, converted: showConverted })
      .then((value) => { if (!disposed) setPreview(value); })
      .catch((reason: unknown) => { if (!disposed) setError(errorMessage(reason, "图片预览失败")); })
      .finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; };
  }, [assetId, unavailable, showConverted, convertedPath, revision]);

  return (
    <section className="preview-panel" aria-label="图片预览">
      <div className="preview-heading"><h2>图片预览</h2><div className="segmented" role="group" aria-label="预览版本"><button aria-pressed={!showConverted} onClick={() => setConverted(false)} disabled={!asset}>原图</button><button aria-pressed={showConverted} onClick={() => setConverted(true)} disabled={!hasConverted}>转换后</button></div></div>
      <div className="preview-filename" title={showConverted ? result?.pdfPath ?? "" : asset?.path ?? ""}>{asset?.relativePath ?? "未选择图片"}</div>
      <div className="preview-surface" aria-busy={loading}>
        {loading && <div className="preview-placeholder" role="status"><LoaderCircle size={23} className="spin" aria-hidden="true" /><span>正在载入预览</span></div>}
        {!loading && error && <div className="preview-placeholder preview-error" role="alert"><TriangleAlert size={24} aria-hidden="true" /><span>{error}</span></div>}
        {!loading && !error && unavailable && <div className="preview-placeholder"><FileImage size={32} strokeWidth={1.3} aria-hidden="true" /><span>{asset ? asset.message || "此图片暂不可预览" : "未选择图片"}</span></div>}
        {!loading && !error && preview?.kind === "image" && <div className="image-stage"><img src={preview.data} alt={`${showConverted ? "转换后" : "原始"}：${asset?.relativePath ?? ""}`} onError={() => setError("图片无法显示")} /></div>}
        {!loading && !error && preview?.kind === "pdf" && <Suspense fallback={<div className="preview-placeholder"><LoaderCircle size={23} className="spin" aria-hidden="true" /><span>正在载入 PDF</span></div>}><PdfPreview key={`${assetId}:${showConverted}:${revision}`} data={preview.data} label={asset?.relativePath ?? "PDF"} /></Suspense>}
      </div>
      <div className="preview-metadata"><span>{showConverted ? "PDF" : asset?.format.toUpperCase() ?? "无图片"}</span>{!showConverted && asset?.width && asset.height ? <span>{asset.width} × {asset.height} px</span> : null}<span>{asset ? formatBytes(showConverted ? result?.bytes ?? null : asset.bytes) : ""}</span></div>
    </section>
  );
}
