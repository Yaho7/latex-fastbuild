import { CircleCheck, FileImage, FileWarning, Search } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Separator } from "@/components/ui/separator";
import { Table, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
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
  return (
    <section className="asset-panel @container flex min-h-0 min-w-0 flex-1 flex-col bg-background" aria-label="项目图片">
      <header className="flex shrink-0 flex-col gap-3 px-4 py-3">
        <div className="flex items-center gap-2">
          <h2 className="text-sm font-semibold">项目图片</h2>
          <Badge variant="secondary">{props.total}</Badge>
        </div>
        <div className="flex min-w-0 items-center gap-2">
          <InputGroup className="min-w-0 flex-1">
            <InputGroupInput type="search" aria-label="搜索图片" placeholder="搜索路径或格式" value={props.search} onChange={(event) => props.onSearch(event.target.value)} />
            <InputGroupAddon><Search aria-hidden="true" /></InputGroupAddon>
          </InputGroup>
          <NativeSelect aria-label="筛选状态" value={props.filter} onChange={(event) => props.onFilter(event.target.value)}>
            <NativeSelectOption value="all">全部状态</NativeSelectOption>
            <NativeSelectOption value="ready">可转换</NativeSelectOption>
            <NativeSelectOption value="converted">已转换</NativeSelectOption>
            <NativeSelectOption value="error">转换失败</NativeSelectOption>
            <NativeSelectOption value="missing">文件缺失</NativeSelectOption>
            <NativeSelectOption value="ambiguous">路径不明确</NativeSelectOption>
            <NativeSelectOption value="unsupported">不支持</NativeSelectOption>
            <NativeSelectOption value="pdf">PDF</NativeSelectOption>
          </NativeSelect>
        </div>
      </header>
      <Separator />
      <div className="min-h-0 flex-1 overflow-auto">
        <Table className="table-fixed">
          <TableCaption className="sr-only">项目图片及转换状态</TableCaption>
          <TableHeader>
            <TableRow>
              <TableHead className="w-11 pl-4">
                <Checkbox aria-label="选择当前列表" checked={allChecked ? true : checkedCount > 0 ? "indeterminate" : false} disabled={props.busy || readyIds.length === 0} onCheckedChange={() => props.onSelectAll(readyIds, !allChecked)} />
              </TableHead>
              <TableHead>图片</TableHead>
              <TableHead className="hidden w-28 @min-[36rem]:table-cell">尺寸</TableHead>
              <TableHead className="w-20 text-right">大小</TableHead>
              <TableHead className="w-28 pr-4 text-right">状态</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {props.images.map((asset) => {
              const result = props.results[asset.id];
              const converted = Boolean(result?.pdfPath && !result.error);
              const warning = ["missing", "unsupported", "ambiguous"].includes(asset.status);
              const label = result?.cancelled ? "已取消" : result?.error ? "转换失败" : converted ? (result?.cached ? "已缓存" : "已转换") : statusLabels[asset.status];
              const normalizedPath = asset.relativePath.replaceAll("\\", "/");
              const slash = normalizedPath.lastIndexOf("/");
              const filename = normalizedPath.slice(slash + 1);
              const folder = slash < 0 ? "." : normalizedPath.slice(0, slash);
              return (
                <TableRow key={asset.id} data-state={props.activeId === asset.id ? "selected" : undefined} className="h-[76px]">
                  <TableCell className="pl-4">
                    <Checkbox aria-label={`选择 ${asset.relativePath}`} checked={props.selected.has(asset.id)} disabled={props.busy || asset.status !== "ready"} onCheckedChange={() => props.onSelect(asset.id)} />
                  </TableCell>
                  <TableCell className="min-w-0 whitespace-normal">
                    <div className="flex min-w-0 flex-col gap-1">
                      <Button variant="ghost" className="h-auto min-w-0 justify-start px-0 py-0" aria-label={`预览 ${asset.relativePath}`} aria-pressed={props.activeId === asset.id} onClick={() => props.onActivate(asset.id)} title={asset.relativePath}>
                        {warning ? <FileWarning data-icon="inline-start" aria-hidden="true" /> : <FileImage data-icon="inline-start" aria-hidden="true" />}
                        <span className="truncate">{filename}</span>
                      </Button>
                      <p className="truncate text-xs text-muted-foreground" title={asset.relativePath}>{folder}</p>
                      <p className="text-xs text-muted-foreground">{asset.format.toUpperCase()} · {asset.references.length} 处引用</p>
                      {result?.error && !result.cancelled && <p className="break-words text-xs text-destructive">{result.error}</p>}
                      {asset.message && <p className="break-words text-xs text-muted-foreground">{asset.message}</p>}
                    </div>
                  </TableCell>
                  <TableCell className="hidden @min-[36rem]:table-cell"><span className="text-xs tabular-nums text-muted-foreground">{asset.width && asset.height ? `${asset.width} × ${asset.height}` : "-"}</span></TableCell>
                  <TableCell className="text-right"><span className="text-xs tabular-nums text-muted-foreground">{formatBytes(asset.bytes)}</span></TableCell>
                  <TableCell className="pr-4 text-right">
                    <Badge variant={result?.error && !result.cancelled ? "destructive" : converted ? "success" : warning ? "warning" : "secondary"} title={result?.pdfPath ?? undefined}>
                      {converted && <CircleCheck data-icon="inline-start" aria-hidden="true" />}{label}
                    </Badge>
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
        {props.images.length === 0 && <Empty><EmptyHeader><EmptyTitle>{props.total ? "没有匹配的图片" : "未发现图片引用"}</EmptyTitle></EmptyHeader></Empty>}
      </div>
      <Separator />
      <footer className="flex shrink-0 items-center justify-between gap-3 px-4 py-2 text-xs text-muted-foreground">
        <span>{props.images.length} / {props.total} 张图片</span>
        <span>已选 {props.selected.size} 张</span>
      </footer>
    </section>
  );
}
