import { useEffect, useRef } from "react";
import { ArrowRight, LoaderCircle, X } from "lucide-react";
import type { ChangePreview } from "./types";
import { relativeFile } from "./utils";

interface Props {
  preview: ChangePreview;
  root: string;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onApply: () => void;
}

export default function ReviewDialog(props: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const busyRef = useRef(props.busy);
  busyRef.current = props.busy;
  const closeRef = useRef(props.onClose);
  closeRef.current = props.onClose;
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialog = ref.current;
    dialog?.querySelector<HTMLButtonElement>("button")?.focus();
    function keydown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busyRef.current) closeRef.current();
      }
      if (event.key !== "Tab" || !dialog) return;
      const elements = [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), [tabindex="0"]')];
      const first = elements[0];
      const last = elements.at(-1);
      if (!first) { event.preventDefault(); dialog.focus(); return; }
      if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && (document.activeElement === last || !dialog.contains(document.activeElement))) { event.preventDefault(); first.focus(); }
    }
    document.addEventListener("keydown", keydown);
    return () => { document.removeEventListener("keydown", keydown); previous?.focus(); };
  }, []);

  return (
    <div className="modal-backdrop">
      <div ref={ref} role="dialog" aria-modal="true" aria-labelledby="review-title" aria-describedby="review-summary" tabIndex={-1} className="review-dialog">
        <header className="modal-header"><div><h2 id="review-title">确认引用替换</h2><p id="review-summary">{props.preview.files.length} 个文件，{props.preview.referenceCount} 处引用</p></div><button className="icon-button" aria-label="关闭替换预览" title="关闭替换预览" disabled={props.busy} onClick={props.onClose}><X size={18} aria-hidden="true" /></button></header>
        <div className="diff-scroll">
          {props.preview.files.length === 0 && <p className="diff-empty">没有可替换的引用</p>}
          {props.preview.files.map((file) => <section className="diff-file" key={file.path}><h3 title={file.path}>{relativeFile(file.path, props.root)}</h3><div className="diff-column-labels"><span>替换前</span><span>替换后</span></div>{file.changes.map((change, index) => <div className="diff-change" key={`${change.line}:${index}`}><span className="diff-line">第 {change.line} 行</span><pre className="diff-before"><code>{change.before}</code></pre><pre className="diff-after"><code>{change.after}</code></pre></div>)}</section>)}
        </div>
        {props.error && <div className="modal-error" role="alert">{props.error}</div>}
        <footer className="modal-footer"><button className="button" disabled={props.busy} onClick={props.onClose}>返回</button><button className="button primary" disabled={props.busy || props.preview.referenceCount === 0 || props.preview.files.length === 0} onClick={props.onApply}>{props.busy ? <LoaderCircle size={16} className="spin" aria-hidden="true" /> : <ArrowRight size={16} aria-hidden="true" />}应用替换</button></footer>
      </div>
    </div>
  );
}
