import { ArrowRight, FileCheck2, LoaderCircle, Undo2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
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
    <aside className="flex h-[340px] min-w-0 shrink-0 flex-col bg-background" aria-label="转换设置">
      <Tabs defaultValue="settings" className="min-h-0 flex-1 gap-0">
        <TabsList variant="line" className="mx-4 mt-1 shrink-0" aria-label="转换设置与引用位置">
          <TabsTrigger value="settings">转换设置</TabsTrigger>
          <TabsTrigger value="references">引用位置 <Badge variant="secondary">{props.activeAsset?.references.length ?? 0}</Badge></TabsTrigger>
        </TabsList>
        <TabsContent value="settings" className="min-h-0 overflow-y-auto px-4 py-3">
          <FieldSet disabled={props.busy} className="gap-3">
            <FieldLegend className="sr-only">画质</FieldLegend>
            <FieldGroup className="gap-3">
              <Field className="gap-2">
                <FieldLabel id="quality-label" className="sr-only">画质</FieldLabel>
                <ToggleGroup type="single" variant="outline" size="sm" value={props.quality} disabled={props.busy} aria-labelledby="quality-label" className="w-full" onValueChange={(value) => { if (value) props.onQuality(value as Quality); }}>
                  {([["original", "原始"], ["high", "高质量"], ["compact", "紧凑"], ["custom", "自定义"]] as const).map(([quality, label]) => (
                    <ToggleGroupItem key={quality} value={quality} className="min-w-0 flex-1">{label}</ToggleGroupItem>
                  ))}
                </ToggleGroup>
              </Field>
              {props.quality === "custom" ? (
                <FieldGroup className="grid grid-cols-3 gap-2">
                  <Field className="min-w-0 gap-1.5" data-disabled={props.busy} data-invalid={Boolean(validation)}>
                    <FieldLabel htmlFor="conversion-dpi">分辨率 (DPI)</FieldLabel>
                    <Input id="conversion-dpi" type="number" min="72" max="1200" step="1" disabled={props.busy} aria-invalid={Boolean(validation)} aria-describedby={validation ? "conversion-settings-error" : undefined} value={props.custom.dpi} onChange={(event) => props.onCustom({ ...props.custom, dpi: event.target.value })} />
                  </Field>
                  <Field className="min-w-0 gap-1.5" data-disabled={props.busy} data-invalid={Boolean(validation)}>
                    <FieldLabel htmlFor="conversion-width">参考宽度 (mm)</FieldLabel>
                    <Input id="conversion-width" type="number" min="10" max="1000" step="0.1" disabled={props.busy} aria-invalid={Boolean(validation)} aria-describedby={validation ? "conversion-settings-error" : undefined} value={props.custom.maxWidthMm} onChange={(event) => props.onCustom({ ...props.custom, maxWidthMm: event.target.value })} />
                  </Field>
                  <Field className="min-w-0 gap-1.5" data-disabled={props.busy} data-invalid={Boolean(validation)}>
                    <FieldLabel htmlFor="conversion-jpeg">JPEG 质量 (%)</FieldLabel>
                    <Input id="conversion-jpeg" type="number" min="1" max="100" step="1" disabled={props.busy} aria-invalid={Boolean(validation)} aria-describedby={validation ? "conversion-settings-error" : undefined} value={props.custom.jpegQuality} onChange={(event) => props.onCustom({ ...props.custom, jpegQuality: event.target.value })} />
                  </Field>
                </FieldGroup>
              ) : (
                <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
                  <div className="flex flex-col gap-1"><dt className="text-muted-foreground">像素</dt><dd>{props.quality === "original" ? "保持原始 / 无损" : `${preset.dpi} DPI`}</dd></div>
                  <div className="flex flex-col gap-1"><dt className="text-muted-foreground">参考宽度</dt><dd>{props.quality === "original" ? "不降采样" : `${preset.maxWidthMm} mm`}</dd></div>
                  {props.quality !== "original" && <div className="flex flex-col gap-1"><dt className="text-muted-foreground">JPEG 质量</dt><dd>{preset.jpegQuality}%</dd></div>}
                </dl>
              )}
              <FieldDescription>SVG 矢量保留 · 透明通道保留</FieldDescription>
              {validation && <FieldError id="conversion-settings-error">{validation}</FieldError>}
            </FieldGroup>
          </FieldSet>
        </TabsContent>
        <TabsContent value="references" className="min-h-0 overflow-y-auto px-4 py-3">
          <section aria-label="图片引用位置">
            {props.activeAsset?.references.length ? (
              <ul className="flex flex-col gap-3">
                {props.activeAsset.references.map((ref, index) => (
                  <li key={`${ref.file}:${ref.start}:${index}`} className="flex min-w-0 items-center justify-between gap-3 text-xs">
                    <span className="truncate" title={ref.file}>{relativeFile(ref.file, props.root)}</span>
                    <span className="shrink-0 tabular-nums text-muted-foreground">第 {ref.line} 行</span>
                  </li>
                ))}
              </ul>
            ) : <Empty className="p-4"><EmptyHeader><EmptyTitle>{props.activeAsset ? "无引用位置" : "未选择图片"}</EmptyTitle></EmptyHeader></Empty>}
          </section>
        </TabsContent>
      </Tabs>
      <Separator />
      <div className="flex shrink-0 flex-col gap-2 px-4 py-3">
        <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground"><span>所选图片</span><span>{props.selectedCount} 张</span></div>
        <Button className="w-full" aria-label="转换所选图片" disabled={props.busy || props.selectedCount === 0 || Boolean(validation)} onClick={props.onConvert}>
          {props.converting ? <LoaderCircle data-icon="inline-start" className="animate-spin" aria-hidden="true" /> : <ArrowRight data-icon="inline-start" aria-hidden="true" />}
          {props.converting ? "正在转换" : "转换所选图片"}
        </Button>
        <section className="grid grid-cols-2 gap-2" aria-label="引用替换">
          <Button variant="outline" size="sm" disabled={props.busy || !props.canReview} onClick={props.onReview}><FileCheck2 data-icon="inline-start" aria-hidden="true" />预览替换</Button>
          <Button variant="ghost" size="sm" disabled={props.busy || !props.undoAvailable} onClick={props.onUndo}><Undo2 data-icon="inline-start" aria-hidden="true" />撤销替换</Button>
        </section>
      </div>
    </aside>
  );
}
