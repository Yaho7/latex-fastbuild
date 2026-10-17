import { useRef, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Check, FileCode2, FolderOpen, LoaderCircle, RefreshCw, Square, TriangleAlert, X } from "lucide-react";
import AssetList from "./AssetList";
import AssetPreview from "./AssetPreview";
import ConversionPanel, { conversionOptions, settingsError } from "./ConversionPanel";
import type { CustomSettings } from "./ConversionPanel";
import ReviewDialog from "./ReviewDialog";
import Splitter from "./Splitter";
import type { ChangePreview, ConversionProgress, ConversionResult, ImageResult, Project, Quality, ReplacementResult } from "./types";
import { errorMessage, fileName } from "./utils";

type Operation = "opening" | "converting" | "reviewing" | "applying" | "undoing";
const operationLabels: Record<Operation, string> = { opening: "正在扫描项目", converting: "正在转换图片", reviewing: "正在读取引用变更", applying: "正在替换引用", undoing: "正在恢复引用" };

export default function App() {
  const [project, setProject] = useState<Project | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [activeId, setActiveId] = useState<string | null>(null);
  const [results, setResults] = useState<Record<string, ImageResult>>({});
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState("all");
  const [quality, setQuality] = useState<Quality>("original");
  const [custom, setCustom] = useState<CustomSettings>({ dpi: "300", maxWidthMm: "160", jpegQuality: "90" });
  const [operation, setOperation] = useState<Operation | null>(null);
  const operationRef = useRef<Operation | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("就绪");
  const [progress, setProgress] = useState<ConversionProgress | null>(null);
  const [cancelRequested, setCancelRequested] = useState(false);
  const cancelRef = useRef(false);
  const [review, setReview] = useState<ChangePreview | null>(null);
  const [revision, setRevision] = useState(0);
  const [assetsWidth, setAssetsWidth] = useState(360);
  const [settingsWidth, setSettingsWidth] = useState(272);
  const busy = operation !== null;

  async function execute(kind: Operation, action: () => Promise<void>) {
    if (operationRef.current) return;
    operationRef.current = kind;
    setOperation(kind);
    setError(null);
    try { await action(); }
    catch (reason) { setError(errorMessage(reason)); }
    finally { operationRef.current = null; setOperation(null); }
  }

  function clearAssetState() {
    setSelected(new Set());
    setResults({});
    setActiveId(null);
    setReview(null);
    setProgress(null);
    setRevision((value) => value + 1);
  }

  async function loadProject(mainFile: string, message?: string) {
    const next = await invoke<Project>("open_project", { mainFile });
    clearAssetState();
    setProject(next);
    setActiveId(next.images.find((image) => image.status === "ready")?.id ?? next.images[0]?.id ?? null);
    setSearch("");
    setFilter("all");
    setNotice(message ?? `已扫描 ${next.texFiles.length} 个 TeX 文件，${next.images.length} 张图片`);
  }

  function openProject() {
    void execute("opening", async () => {
      const mainFile = await open({ multiple: false, directory: false, title: "打开 LaTeX 主文件", filters: [{ name: "LaTeX", extensions: ["tex"] }] });
      if (typeof mainFile === "string") await loadProject(mainFile);
    });
  }

  function convert() {
    if (!project || selected.size === 0 || settingsError(quality, custom)) return;
    const imageIds = project.images.filter((image) => image.status === "ready" && selected.has(image.id)).map((image) => image.id);
    if (!imageIds.length) return;
    void execute("converting", async () => {
      setReview(null);
      setProgress(null);
      cancelRef.current = false;
      setCancelRequested(false);
      setResults((previous) => Object.fromEntries(Object.entries(previous).filter(([id]) => !imageIds.includes(id))));
      let acceptingEvents = true;
      const unlisten = await listen<ConversionProgress>("conversion-progress", ({ payload }) => {
        if (acceptingEvents) setProgress(payload);
      });
      try {
        const result = await invoke<ConversionResult>("convert_images", { options: conversionOptions(quality, custom, imageIds) });
        const next: Record<string, ImageResult> = {};
        for (const image of result.images) next[image.id] = { ...image, error: image.error || (!image.pdfPath ? "未生成 PDF" : null) };
        for (const id of imageIds) {
          if (!next[id]) next[id] = { id, pdfPath: null, bytes: null, cached: false, error: result.cancelled ? null : "未返回转换结果", cancelled: result.cancelled };
        }
        setResults((previous) => ({ ...previous, ...next }));
        const succeeded = result.images.filter((image) => image.pdfPath && !image.error).length;
        const failed = Object.values(next).filter((image) => image.error).length;
        setNotice(result.cancelled ? `已取消：${succeeded} 张已完成` : `转换完成：${succeeded} 成功，${failed} 失败`);
        setRevision((value) => value + 1);
      } finally {
        acceptingEvents = false;
        unlisten();
        cancelRef.current = false;
        setCancelRequested(false);
      }
    });
  }

  async function cancelConversion() {
    if (operationRef.current !== "converting" || cancelRef.current) return;
    cancelRef.current = true;
    setCancelRequested(true);
    try { await invoke("cancel_conversion"); }
    catch (reason) {
      if (operationRef.current === "converting") {
        cancelRef.current = false;
        setCancelRequested(false);
        setError(errorMessage(reason, "取消失败，请重试"));
      }
    }
  }

  function previewChanges() {
    void execute("reviewing", async () => { setReview(await invoke<ChangePreview>("preview_changes")); });
  }

  function replace(undo: boolean) {
    if (!project || (!undo && !review)) return;
    const mainFile = project.mainFile;
    void execute(undo ? "undoing" : "applying", async () => {
      const result = await invoke<ReplacementResult>(undo ? "undo_replacements" : "apply_replacements");
      const message = `${undo ? "已恢复" : "已替换"} ${result.filesChanged} 个文件中的 ${result.referencesChanged} 处引用`;
      // The backend changed references; drop all pre-mutation previews and conversions.
      clearAssetState();
      setProject((previous) => previous ? { ...previous, undoAvailable: !undo } : null);
      setNotice(message);
      try { await loadProject(mainFile, message); }
      catch (reason) { setError(`${undo ? "引用已恢复" : "引用已替换"}，但重新扫描失败：${errorMessage(reason)}`); }
    });
  }

  const activeAsset = project?.images.find((image) => image.id === activeId) ?? null;
  const visibleImages = (project?.images ?? []).filter((image) => {
    const textMatch = `${image.relativePath} ${image.format}`.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase());
    const result = results[image.id];
    const stateMatch = filter === "all" || (filter === "converted" ? Boolean(result?.pdfPath && !result.error) : filter === "error" ? Boolean(result?.error) : image.status === filter);
    return textMatch && stateMatch;
  });
  const canReview = Object.values(results).some((result) => result.pdfPath && !result.error);
  const missingCount = project?.images.filter((image) => image.status === "missing").reduce((sum, image) => sum + image.references.length, 0) ?? 0;
  const progressValue = progress && progress.total > 0 ? Math.min(100, Math.max(0, progress.current / progress.total * 100)) : 0;
  const layoutStyle = { "--assets-width": `${assetsWidth}px`, "--settings-width": `${settingsWidth}px` } as CSSProperties;

  return <div className="app-shell">
    <div className="workbench" inert={review !== null}>
      <header className="app-toolbar">
        <div className="brand"><FileCode2 size={23} strokeWidth={1.7} aria-hidden="true" /><span>texgo</span><span className="version">0.2.0</span></div>
        <span className="toolbar-divider" />
        <div className="project-title"><strong title={project?.mainFile}>{project ? fileName(project.mainFile) : "LaTeX 图片工作台"}</strong>{project && <span title={project.root}>{project.root}</span>}</div>
        <div className="toolbar-actions"><button className="button" disabled={busy} onClick={openProject}><FolderOpen size={16} aria-hidden="true" />打开文件</button><button className="icon-button" aria-label="重新扫描" title="重新扫描" disabled={busy || !project} onClick={() => { if (project) void execute("opening", () => loadProject(project.mainFile)); }}><RefreshCw size={16} aria-hidden="true" /></button></div>
      </header>
      {error && !review && <div className="error-banner" role="alert"><TriangleAlert size={17} aria-hidden="true" /><span>{error}</span><button className="icon-button" aria-label="关闭错误提示" title="关闭错误提示" onClick={() => setError(null)}><X size={16} aria-hidden="true" /></button></div>}
      {project && (missingCount > 0 || project.warnings.length > 0) && <details className="project-warnings"><summary><TriangleAlert size={15} aria-hidden="true" /><span>{missingCount > 0 ? `${missingCount} 个图片引用缺失` : "项目扫描警告"}</span>{project.warnings.length > 0 && <span className="warning-count">{project.warnings.length} 条警告</span>}</summary><ul>{project.warnings.map((warning, index) => <li key={index}>{warning}</li>)}{missingCount > 0 && project.warnings.length === 0 && project.images.filter((image) => image.status === "missing").map((image) => <li key={image.id}>{image.relativePath}：{image.message || "文件不存在"}</li>)}</ul></details>}
      {!project ? <main className="empty-workspace"><div className="empty-content"><FileCode2 size={46} strokeWidth={1.2} aria-hidden="true" /><h1>尚未打开项目</h1><button className="button primary" disabled={busy} onClick={openProject}>{busy ? <LoaderCircle size={16} className="spin" aria-hidden="true" /> : <FolderOpen size={17} aria-hidden="true" />}打开主文件</button><span className="file-extension">.tex</span></div></main> : <main className="workspace-grid" style={layoutStyle}>
        <AssetList images={visibleImages} total={project.images.length} selected={selected} activeId={activeId} results={results} busy={busy} search={search} filter={filter} onSearch={setSearch} onFilter={setFilter}
          onActivate={setActiveId}
          onSelect={(id) => { if (!busy) setSelected((previous) => { const next = new Set(previous); if (next.has(id)) next.delete(id); else next.add(id); return next; }); }}
          onSelectAll={(ids, select) => { if (!busy) setSelected((previous) => { const next = new Set(previous); ids.forEach((id) => { if (select) next.add(id); else next.delete(id); }); return next; }); }} />
        <Splitter label="调整图片列表宽度" value={assetsWidth} min={280} max={560} onChange={setAssetsWidth} />
        <AssetPreview key={`${revision}:${activeAsset?.id ?? "empty"}`} asset={activeAsset} result={activeAsset ? results[activeAsset.id] : undefined} revision={revision} />
        <Splitter label="调整设置栏宽度" value={settingsWidth} min={240} max={380} reverse onChange={setSettingsWidth} />
        <ConversionPanel quality={quality} custom={custom} selectedCount={selected.size} busy={busy} converting={operation === "converting"} canReview={canReview} undoAvailable={project.undoAvailable} activeAsset={activeAsset} root={project.root} onQuality={setQuality} onCustom={setCustom} onConvert={convert} onReview={previewChanges} onUndo={() => replace(true)} />
      </main>}
      <footer className="status-bar"><div className="status-text" role="status" aria-live="polite">{busy ? <LoaderCircle size={14} className="spin" aria-hidden="true" /> : project ? <Check size={14} aria-hidden="true" /> : <span className="status-dot" />}<span>{operation ? cancelRequested ? "正在完成当前图片，随后取消" : operationLabels[operation] : notice}</span>{operation === "converting" && progress && <span className="progress-name" title={progress.name}>{progress.current} / {progress.total}　{progress.name}　{progress.status}</span>}</div>{operation === "converting" && <div className="progress-controls"><div className="progress-track" role="progressbar" aria-label="转换进度" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progressValue}><span style={{ width: `${progressValue}%` }} /></div><button className="button compact" aria-label="取消转换" disabled={cancelRequested} onClick={() => { void cancelConversion(); }}><Square size={12} aria-hidden="true" />{cancelRequested ? "等待取消" : "取消转换"}</button></div>}{!busy && project && <span className="status-file-count">{project.texFiles.length} 个 TeX 文件</span>}</footer>
    </div>
    {review && project && <ReviewDialog preview={review} root={project.root} busy={busy} error={error} onClose={() => { if (!operationRef.current) { setReview(null); setError(null); } }} onApply={() => replace(false)} />}
  </div>;
}
