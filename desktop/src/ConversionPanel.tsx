import { ArrowRight, FileCheck2, LoaderCircle, Undo2 } from "lucide-react";
import type { ConversionOptions, ImageAsset, Quality } from "./types";
import { relativeFile } from "./utils";

export const presets: Record<Quality, { dpi: number; maxWidthMm: number; jpegQuality: number }> = {
  original: { dpi: 300, maxWidthMm: 160, jpegQuality: 100 },
  high: { dpi: 300, maxWidthMm: 160, jpegQuality: 92 },
  compact: { dpi: 150, maxWidthMm: 160, jpegQuality: 75 },
  custom: { dpi: 300, maxWidthMm: 160, jpegQuality: 90 },
};

export interface CustomSettings {
  dpi: string;
  maxWidthMm: string;
  jpegQuality: string;
}

export function settingsError(quality: Quality, custom: CustomSettings): string | null {
  if (quality !== "custom") return null;
  const dpi = Number(custom.dpi);
  const width = Number(custom.maxWidthMm);
  const jpeg = Number(custom.jpegQuality);
  if (!custom.dpi.trim() || !Number.isInteger(dpi) || dpi < 72 || dpi > 1200) return "DPI 需为 72–1200 的整数";
  if (!custom.maxWidthMm.trim() || !Number.isFinite(width) || width < 10 || width > 1000) return "参考宽度需为 10–1000 mm";
  if (!custom.jpegQuality.trim() || !Number.isInteger(jpeg) || jpeg < 1 || jpeg > 100) return "JPEG 质量需为 1–100 的整数";
  return null;
}

export function conversionOptions(quality: Quality, custom: CustomSettings, imageIds: string[]): ConversionOptions {
  return {
    imageIds,
    quality,
    ...(quality === "custom" ? { dpi: Number(custom.dpi), maxWidthMm: Number(custom.maxWidthMm), jpegQuality: Number(custom.jpegQuality) } : presets[quality]),
  };
}

interface Props {
  quality: Quality;
  custom: CustomSettings;
  selectedCount: number;
  busy: boolean;
  converting: boolean;
  canReview: boolean;
  undoAvailable: boolean;
  activeAsset: ImageAsset | null;
  root: string;
  onQuality: (quality: Quality) => void;
  onCustom: (custom: CustomSettings) => void;
  onConvert: () => void;
  onReview: () => void;
  onUndo: () => void;
}

export default function ConversionPanel(props: Props) {
  const validation = settingsError(props.quality, props.custom);
  const preset = presets[props.quality];
  return (
    <aside className="conversion-panel" aria-label="转换设置">
      <div className="panel-heading"><h2>转换设置</h2><span className="output-format">PDF</span></div>
      <div className="settings-scroll">
        <fieldset className="quality-fieldset" disabled={props.busy}>
          <legend>画质</legend>
          <div className="quality-options">
            {([["original", "原始"], ["high", "高质量"], ["compact", "紧凑"], ["custom", "自定义"]] as const).map(([quality, label]) => (
              <label key={quality} className={props.quality === quality ? "selected" : ""}><input type="radio" name="quality" value={quality} checked={props.quality === quality} onChange={() => props.onQuality(quality)} /><span>{label}</span></label>
            ))}
          </div>
          {props.quality === "custom" ? (
            <div className="custom-fields">
              <label>分辨率 (DPI)<input type="number" min="72" max="1200" step="1" value={props.custom.dpi} onChange={(event) => props.onCustom({ ...props.custom, dpi: event.target.value })} /></label>
              <label>参考宽度 (mm)<input type="number" min="10" max="1000" step="0.1" value={props.custom.maxWidthMm} onChange={(event) => props.onCustom({ ...props.custom, maxWidthMm: event.target.value })} /></label>
              <label>JPEG 质量 (%)<input type="number" min="1" max="100" step="1" value={props.custom.jpegQuality} onChange={(event) => props.onCustom({ ...props.custom, jpegQuality: event.target.value })} /></label>
            </div>
          ) : (
            <dl className="preset-summary">
              <div><dt>像素</dt><dd>{props.quality === "original" ? "保持原始 / 无损" : `${preset.dpi} DPI`}</dd></div>
              <div><dt>参考宽度</dt><dd>{props.quality === "original" ? "不降采样" : `${preset.maxWidthMm} mm`}</dd></div>
              {props.quality !== "original" && <div><dt>JPEG 质量</dt><dd>{preset.jpegQuality}%</dd></div>}
            </dl>
          )}
          <p className="parameter-note">SVG 保持矢量与源页面尺寸；透明图片使用无损压缩。</p>
          {validation && <p className="field-error" role="alert">{validation}</p>}
        </fieldset>
        <div className="conversion-actions">
          <div className="section-label"><span>所选图片</span><strong>{props.selectedCount} 张</strong></div>
          <button className="button primary full-width" aria-label="转换所选图片" disabled={props.busy || props.selectedCount === 0 || Boolean(validation)} onClick={props.onConvert}>{props.converting ? <LoaderCircle size={16} className="spin" aria-hidden="true" /> : <ArrowRight size={16} aria-hidden="true" />}<span>{props.converting ? "正在转换" : "转换所选图片"}</span></button>
        </div>
        <section className="replacement-section" aria-label="引用替换">
          <h3>引用替换</h3>
          <button className="button full-width" disabled={props.busy || !props.canReview} onClick={props.onReview}><FileCheck2 size={16} aria-hidden="true" />预览替换</button>
          <button className="button quiet full-width" disabled={props.busy || !props.undoAvailable} onClick={props.onUndo}><Undo2 size={15} aria-hidden="true" />撤销替换</button>
        </section>
        <section className="references-section" aria-label="图片引用位置">
          <h3>引用位置<span className="count">{props.activeAsset?.references.length ?? 0}</span></h3>
          {props.activeAsset?.references.length ? <ul className="reference-list">{props.activeAsset.references.map((ref, index) => <li key={`${ref.file}:${ref.start}:${index}`}><span title={ref.file}>{relativeFile(ref.file, props.root)}</span><span>第 {ref.line} 行</span></li>)}</ul> : <p className="muted">{props.activeAsset ? "无引用位置" : "未选择图片"}</p>}
        </section>
      </div>
    </aside>
  );
}
