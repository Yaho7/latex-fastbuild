import { useEffect, useRef } from "react";
import { CircleCheck, FileImage, FileWarning, Search } from "lucide-react";
import type { AssetStatus, ImageAsset, ImageResult } from "./types";
import { formatBytes } from "./utils";

export const statusLabels: Record<AssetStatus, string> = {
  ready: "待转换",
  missing: "文件缺失",
  unsupported: "不支持",
  ambiguous: "路径不明确",
  pdf: "PDF",
};

interface Props {
  images: ImageAsset[];
  total: number;
  selected: Set<string>;
  activeId: string | null;
  results: Record<string, ImageResult>;
  busy: boolean;
  search: string;
  filter: string;
  onSearch: (value: string) => void;
  onFilter: (value: string) => void;
  onSelect: (id: string) => void;
  onSelectAll: (ids: string[], select: boolean) => void;
  onActivate: (id: string) => void;
}

export default function AssetList(props: Props) {
  const readyIds = props.images.filter((image) => image.status === "ready").map((image) => image.id);
  const checkedCount = readyIds.filter((id) => props.selected.has(id)).length;
  const allChecked = readyIds.length > 0 && checkedCount === readyIds.length;
  const selectAllRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (selectAllRef.current) selectAllRef.current.indeterminate = checkedCount > 0 && !allChecked;
  }, [allChecked, checkedCount]);

  return (
    <section className="asset-panel" aria-label="项目图片">
      <div className="panel-heading"><h2>项目图片</h2><span className="count">{props.total}</span></div>
      <div className="asset-filters">
        <label className="search-field"><Search size={15} aria-hidden="true" /><input type="search" aria-label="搜索图片" placeholder="搜索路径或格式" value={props.search} onChange={(event) => props.onSearch(event.target.value)} /></label>
        <select aria-label="筛选状态" value={props.filter} onChange={(event) => props.onFilter(event.target.value)}>
          <option value="all">全部状态</option>
          <option value="ready">可转换</option>
          <option value="converted">已转换</option>
          <option value="error">转换失败</option>
          <option value="missing">文件缺失</option>
          <option value="ambiguous">路径不明确</option>
          <option value="unsupported">不支持</option>
          <option value="pdf">PDF</option>
        </select>
      </div>
      <div className="selection-bar">
        <label><input ref={selectAllRef} type="checkbox" aria-label="选择当前列表" checked={allChecked} disabled={props.busy || readyIds.length === 0} onChange={() => props.onSelectAll(readyIds, !allChecked)} />当前列表</label>
        <span>已选 {props.selected.size} 张</span>
      </div>
      <div className="asset-scroll">
        <table className="asset-table">
          <caption className="sr-only">项目图片及转换状态</caption>
          <colgroup><col className="asset-select-column" /><col /><col className="asset-status-column" /></colgroup>
          <thead className="sr-only"><tr><th>选择</th><th>图片</th><th>状态</th></tr></thead>
          <tbody>
            {props.images.map((asset) => {
              const result = props.results[asset.id];
              const converted = Boolean(result?.pdfPath && !result.error);
              const warning = ["missing", "unsupported", "ambiguous"].includes(asset.status);
              const label = result?.cancelled ? "已取消" : result?.error ? "转换失败" : converted ? (result?.cached ? "已缓存" : "已转换") : statusLabels[asset.status];
              return (
                <tr key={asset.id} className={props.activeId === asset.id ? "active" : ""}>
                  <td className="select-cell"><input type="checkbox" aria-label={`选择 ${asset.relativePath}`} checked={props.selected.has(asset.id)} disabled={props.busy || asset.status !== "ready"} onChange={() => props.onSelect(asset.id)} /></td>
                  <td className="asset-name-cell">
                    <button className="asset-name" aria-label={`预览 ${asset.relativePath}`} aria-pressed={props.activeId === asset.id} onClick={() => props.onActivate(asset.id)} title={asset.relativePath}>
                      {warning ? <FileWarning size={17} className="warning-ink" aria-hidden="true" /> : <FileImage size={17} aria-hidden="true" />}
                      <span className="asset-path">{asset.relativePath}</span>
                    </button>
                    <div className="asset-meta"><span>{asset.format.toUpperCase()}</span><span>{formatBytes(asset.bytes)}</span><span>{asset.references.length} 处引用</span></div>
                    {result?.error && !result.cancelled && <p className="row-error">{result.error}</p>}
                    {asset.message && <p className="row-note">{asset.message}</p>}
                  </td>
                  <td className="asset-state-cell"><span className={`asset-state ${converted ? "success-ink" : warning || result?.error ? "warning-ink" : ""}`} title={result?.pdfPath ?? undefined}>{converted && <CircleCheck size={12} aria-hidden="true" />}{label}</span></td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {props.images.length === 0 && <div className="list-empty">{props.total ? "没有匹配的图片" : "未发现图片引用"}</div>}
      </div>
      <div className="panel-footnote">{props.images.length} / {props.total} 张图片</div>
    </section>
  );
}
