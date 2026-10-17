import { useRef } from "react";
import { ArrowRight, FileWarning, LoaderCircle, X } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Separator } from "@/components/ui/separator";
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
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog open onOpenChange={(open) => { if (!open && !props.busy) props.onClose(); }}>
      <DialogContent
        showCloseButton={false}
        className="flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl"
        onOpenAutoFocus={() => { returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null; }}
        onCloseAutoFocus={(event) => { event.preventDefault(); returnFocus.current?.focus(); }}
        onEscapeKeyDown={(event) => { if (props.busy) event.preventDefault(); }}
        onInteractOutside={(event) => { if (props.busy) event.preventDefault(); }}
      >
        <div className="flex shrink-0 items-start justify-between gap-4 p-5">
          <DialogHeader>
            <DialogTitle>确认引用替换</DialogTitle>
            <DialogDescription>{props.preview.files.length} 个文件，{props.preview.referenceCount} 处引用</DialogDescription>
          </DialogHeader>
          <DialogClose asChild>
            <Button variant="ghost" size="icon" aria-label="关闭替换预览" title="关闭替换预览" disabled={props.busy}><X aria-hidden="true" /></Button>
          </DialogClose>
        </div>
        <Separator />
        <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-5">
          {props.preview.files.length === 0 && <Empty><EmptyHeader><EmptyTitle>没有可替换的引用</EmptyTitle></EmptyHeader></Empty>}
          {props.preview.files.map((file) => (
            <section className="flex min-w-0 flex-col gap-3" key={file.path}>
              <div className="flex min-w-0 items-center gap-2">
                <h3 className="truncate text-sm font-medium" title={file.path}>{relativeFile(file.path, props.root)}</h3>
                <Badge variant="secondary">{file.changes.length}</Badge>
              </div>
              <div className="grid grid-cols-2 gap-3 text-xs text-muted-foreground"><span>替换前</span><span>替换后</span></div>
              {file.changes.map((change, index) => (
                <div className="flex min-w-0 flex-col gap-2" key={`${change.line}:${index}`}>
                  <span className="text-xs tabular-nums text-muted-foreground">第 {change.line} 行</span>
                  <div className="grid min-w-0 grid-cols-2 gap-3">
                    <pre className="min-w-0 overflow-x-auto rounded-md border bg-muted/40 p-3 text-xs leading-relaxed" aria-label={`第 ${change.line} 行替换前`}><code>{change.before}</code></pre>
                    <pre className="min-w-0 overflow-x-auto rounded-md border bg-muted p-3 text-xs leading-relaxed" aria-label={`第 ${change.line} 行替换后`}><code>{change.after}</code></pre>
                  </div>
                </div>
              ))}
            </section>
          ))}
        </div>
        {props.error && <div className="shrink-0 px-5 pb-4"><Alert variant="destructive"><FileWarning aria-hidden="true" /><AlertDescription>{props.error}</AlertDescription></Alert></div>}
        <Separator />
        <DialogFooter className="shrink-0 p-5">
          <DialogClose asChild><Button variant="outline" disabled={props.busy}>返回</Button></DialogClose>
          <Button disabled={props.busy || props.preview.referenceCount === 0 || props.preview.files.length === 0} onClick={props.onApply}>
            {props.busy ? <LoaderCircle data-icon="inline-start" className="animate-spin" aria-hidden="true" /> : <ArrowRight data-icon="inline-start" aria-hidden="true" />}应用替换
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
