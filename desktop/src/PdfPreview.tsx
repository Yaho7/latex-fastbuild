import { useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, LoaderCircle, Maximize, Minus, Plus } from "lucide-react";
import { getDocument, GlobalWorkerOptions } from "pdfjs-dist";
import type { RenderTask } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { errorMessage } from "./utils";

GlobalWorkerOptions.workerSrc = workerUrl;

export default function PdfPreview({ data, label }: { data: string; label: string }) {
  const host = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(480);
  const [page, setPage] = useState(1);
  const [pages, setPages] = useState(0);
  const [zoom, setZoom] = useState(1);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    const update = () => { if (element.clientWidth > 0) setWidth(element.clientWidth); };
    update();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", update);
      return () => window.removeEventListener("resize", update);
    }
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    let disposed = false;
    let renderTask: RenderTask | undefined;
    let loadingTask: ReturnType<typeof getDocument> | undefined;
    setLoading(true);
    setError(null);
    // Each render owns its canvas, so cancellation cannot paint onto the next page.
    const canvas = document.createElement("canvas");
    canvas.setAttribute("role", "img");
    canvas.setAttribute("aria-label", `${label}，第 ${page} 页`);
    element.replaceChildren(canvas);
    async function renderPage() {
      try {
        const binary = atob(data);
        const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
        loadingTask = getDocument({ data: bytes, isEvalSupported: false });
        const pdf = await loadingTask.promise;
        if (disposed) return;
        setPages(pdf.numPages);
        if (page > pdf.numPages) { setPage(1); return; }
        const pdfPage = await pdf.getPage(page);
        if (disposed) return;
        const natural = pdfPage.getViewport({ scale: 1 });
        const viewport = pdfPage.getViewport({ scale: Math.max(120, width - 48) / natural.width * zoom });
        const ratio = Math.min(window.devicePixelRatio || 1, 2, Math.sqrt(16_000_000 / (viewport.width * viewport.height)));
        canvas.width = Math.max(1, Math.floor(viewport.width * ratio));
        canvas.height = Math.max(1, Math.floor(viewport.height * ratio));
        canvas.style.width = `${viewport.width}px`;
        canvas.style.height = `${viewport.height}px`;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("无法创建 PDF 画布");
        renderTask = pdfPage.render({ canvas, canvasContext: context, viewport, transform: [ratio, 0, 0, ratio, 0, 0] });
        await renderTask.promise;
        if (!disposed) setLoading(false);
      } catch (reason) {
        if (!disposed) { setError(errorMessage(reason, "PDF 预览失败")); setLoading(false); }
      }
    }
    void renderPage();
    return () => {
      disposed = true;
      renderTask?.cancel();
      if (loadingTask) void loadingTask.destroy().catch(() => undefined);
      canvas.remove();
    };
  }, [data, label, page, width, zoom]);

  return <div className="pdf-preview">
    <div className="pdf-toolbar" role="group" aria-label="PDF 预览工具">
      <button className="icon-button" aria-label="上一页" title="上一页" disabled={page <= 1 || loading} onClick={() => setPage((value) => value - 1)}><ChevronLeft size={16} aria-hidden="true" /></button><span className="page-count">{page} / {pages || "…"}</span><button className="icon-button" aria-label="下一页" title="下一页" disabled={page >= pages || loading} onClick={() => setPage((value) => value + 1)}><ChevronRight size={16} aria-hidden="true" /></button>
      <span className="toolbar-divider" />
      <button className="icon-button" aria-label="缩小" title="缩小" disabled={zoom <= 0.25} onClick={() => setZoom((value) => Math.max(0.25, value - 0.25))}><Minus size={15} aria-hidden="true" /></button><span className="zoom-value">{Math.round(zoom * 100)}%</span><button className="icon-button" aria-label="放大" title="放大" disabled={zoom >= 4} onClick={() => setZoom((value) => Math.min(4, value + 0.25))}><Plus size={15} aria-hidden="true" /></button><button className="icon-button" aria-label="适合宽度" title="适合宽度" onClick={() => setZoom(1)}><Maximize size={15} aria-hidden="true" /></button>
    </div>
    <div className="pdf-viewport"><div ref={host} className="pdf-canvas-host" aria-busy={loading} />{loading && <div className="pdf-render-status" role="status"><LoaderCircle size={16} className="spin" aria-hidden="true" />正在渲染</div>}{error && <p className="preview-error pdf-error" role="alert">{error}</p>}</div>
  </div>;
}
