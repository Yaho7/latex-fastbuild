import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "../src/App";
import type { ConversionResult, Project } from "../src/types";

const imageData = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=";
const secondImageData = "data:image/png;base64,c2Vjb25k";
const reference = { file: "/paper/main.tex", line: 8, original: "figures/alpha.png", start: 40, end: 57 };
const project: Project = {
  mainFile: "/paper/main.tex",
  root: "/paper",
  texFiles: ["/paper/main.tex", "/paper/chapter.tex"],
  warnings: ["第 12 行的图片引用未找到文件"],
  undoAvailable: false,
  images: [
    { id: "a", path: "/paper/figures/alpha.png", relativePath: "figures/alpha.png", format: "png", width: 1200, height: 800, bytes: 120000, status: "ready", message: null, references: [reference] },
    { id: "b", path: "/paper/figures/beta.jpg", relativePath: "figures/beta.jpg", format: "jpeg", width: 900, height: 600, bytes: 80000, status: "ready", message: null, references: [{ ...reference, line: 10, original: "figures/beta.jpg" }] },
    { id: "missing", path: null, relativePath: "figures/lost.png", format: "png", width: null, height: null, bytes: null, status: "missing", message: "文件不存在", references: [{ ...reference, line: 12 }] },
    { id: "unsupported", path: "/paper/animation.gif", relativePath: "animation.gif", format: "gif", width: 20, height: 20, bytes: 500, status: "unsupported", message: "不支持此格式", references: [] },
    { id: "pdf", path: "/paper/plot.pdf", relativePath: "plot.pdf", format: "pdf", width: null, height: null, bytes: 4000, status: "pdf", message: null, references: [] },
  ],
};

const success: ConversionResult = {
  images: [{ id: "a", pdfPath: "/paper/figures/alpha.pdf", bytes: 50000, cached: false, error: null }],
  cancelled: false,
};

type Handler = (args: Record<string, unknown>) => unknown;
let handlers: Record<string, Handler>;
let calls: { command: string; args: Record<string, unknown> }[];
let backendProject: Project;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

beforeEach(() => {
  calls = [];
  backendProject = structuredClone(project);
  handlers = {
    "plugin:dialog|open": () => "/paper/main.tex",
    open_project: () => structuredClone(backendProject),
    asset_preview: () => ({ data: imageData, kind: "image" }),
    convert_images: () => success,
    cancel_conversion: () => undefined,
    preview_changes: () => ({ files: [{ path: "/paper/main.tex", changes: [{ line: 8, before: "\\includegraphics{figures/alpha.png}", after: "\\includegraphics{figures/alpha.pdf}" }] }], referenceCount: 1 }),
    apply_replacements: () => { backendProject.undoAvailable = true; return { filesChanged: 1, referencesChanged: 1 }; },
    undo_replacements: () => { backendProject.undoAvailable = false; return { filesChanged: 1, referencesChanged: 1 }; },
  };
  mockIPC((command, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ command, args });
    const handler = handlers[command];
    if (!handler) throw new Error(`Unexpected IPC command: ${command}`);
    return handler(args);
  }, { shouldMockEvents: true });
});

async function openProject() {
  const user = userEvent.setup();
  render(<App />);
  await user.click(screen.getByRole("button", { name: "打开主文件" }));
  await screen.findByRole("button", { name: "预览 figures/alpha.png" });
  return user;
}

async function convertFirst(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("checkbox", { name: "选择 figures/alpha.png" }));
  await user.click(screen.getByRole("button", { name: "转换所选图片" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "预览替换" })).toBeEnabled());
}

describe("LaTeX 图片工作流", () => {
  it("starts empty and leaves the workspace unchanged when the file dialog is cancelled", async () => {
    handlers["plugin:dialog|open"] = () => null;
    const user = userEvent.setup();
    render(<App />);
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "打开主文件" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "打开主文件" })).toBeEnabled());
    expect(calls.some(({ command }) => command === "open_project")).toBe(false);
    expect(screen.getByText("尚未打开项目")).toBeInTheDocument();
    expect(calls[0]).toMatchObject({ command: "plugin:dialog|open", args: { options: { multiple: false, filters: [{ name: "LaTeX", extensions: ["tex"] }] } } });
  });

  it("scans the chosen main file, warns about missing references, filters and selects only ready images", async () => {
    const user = await openProject();
    expect(calls).toContainEqual({ command: "open_project", args: { mainFile: "/paper/main.tex" } });
    expect(screen.getByText("1 个图片引用缺失")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "选择 figures/lost.png" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "选择 plot.pdf" })).toBeDisabled();
    await user.type(screen.getByRole("searchbox", { name: "搜索图片" }), "beta");
    expect(screen.queryByRole("button", { name: "预览 figures/alpha.png" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: "选择当前列表" }));
    await user.clear(screen.getByRole("searchbox", { name: "搜索图片" }));
    expect(screen.getByRole("checkbox", { name: "选择 figures/beta.jpg" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "选择 figures/alpha.png" })).not.toBeChecked();
    await user.selectOptions(screen.getByRole("combobox", { name: "筛选状态" }), "missing");
    expect(screen.getByRole("button", { name: "预览 figures/lost.png" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "预览 figures/beta.jpg" })).not.toBeInTheDocument();
  });

  it("uses shadcn controls and exposes image references in the inspector tab", async () => {
    const user = await openProject();
    const checkbox = screen.getByRole("checkbox", { name: "选择 figures/alpha.png" });
    expect(checkbox).toHaveAttribute("data-slot", "checkbox");
    await user.click(checkbox);
    expect(screen.getByRole("checkbox", { name: "选择当前列表" })).toBePartiallyChecked();
    await user.click(screen.getByRole("tab", { name: /引用位置/ }));
    expect(screen.getByRole("region", { name: "图片引用位置" })).toHaveTextContent("第 8 行");
    await user.click(screen.getByRole("tab", { name: "转换设置" }));
    expect(screen.getByRole("radio", { name: "原始" })).toHaveAttribute("data-state", "on");
  });

  it("waits for conversion completion even after the final progress event and sends custom settings", async () => {
    const pending = deferred<ConversionResult>();
    handlers.convert_images = () => pending.promise;
    const user = await openProject();
    await user.click(screen.getByRole("checkbox", { name: "选择 figures/alpha.png" }));
    await user.click(screen.getByRole("radio", { name: "自定义" }));
    await user.clear(screen.getByRole("spinbutton", { name: "分辨率 (DPI)" }));
    await user.type(screen.getByRole("spinbutton", { name: "分辨率 (DPI)" }), "240");
    await user.clear(screen.getByRole("spinbutton", { name: "参考宽度 (mm)" }));
    await user.type(screen.getByRole("spinbutton", { name: "参考宽度 (mm)" }), "120");
    await user.clear(screen.getByRole("spinbutton", { name: "JPEG 质量 (%)" }));
    await user.type(screen.getByRole("spinbutton", { name: "JPEG 质量 (%)" }), "85");
    await user.click(screen.getByRole("button", { name: "转换所选图片" }));
    await waitFor(() => expect(calls.some(({ command }) => command === "convert_images")).toBe(true));
    await act(async () => { await emit("conversion-progress", { current: 1, total: 1, name: "alpha.png", status: "完成" }); });
    expect(screen.getByRole("button", { name: "预览替换" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "重新扫描" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "选择 figures/alpha.png" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: "原始" })).toBeDisabled();
    expect(calls).toContainEqual({ command: "convert_images", args: { options: { imageIds: ["a"], quality: "custom", dpi: 240, maxWidthMm: 120, jpegQuality: 85 } } });
    await act(async () => { pending.resolve(success); });
    await waitFor(() => expect(screen.getByRole("button", { name: "预览替换" })).toBeEnabled());
    expect(screen.getByText("转换完成：1 成功，0 失败")).toBeInTheDocument();
    const row = screen.getByRole("button", { name: "预览 figures/alpha.png" }).closest("tr")!;
    expect(within(row).getByText("已转换")).toBeInTheDocument();
  });

  it("validates custom settings before any conversion command", async () => {
    const user = await openProject();
    await user.click(screen.getByRole("checkbox", { name: "选择 figures/alpha.png" }));
    await user.click(screen.getByRole("radio", { name: "自定义" }));
    await user.clear(screen.getByRole("spinbutton", { name: "分辨率 (DPI)" }));
    expect(screen.getByRole("button", { name: "转换所选图片" })).toBeDisabled();
    expect(screen.getByText("DPI 需为 72–1200 的整数")).toBeInTheDocument();
    expect(calls.some(({ command }) => command === "convert_images")).toBe(false);
  });

  it("keeps conversion locked until cancellation finishes and preserves completed image results", async () => {
    const pending = deferred<ConversionResult>();
    handlers.convert_images = () => pending.promise;
    const user = await openProject();
    await user.click(screen.getByRole("checkbox", { name: "选择当前列表" }));
    await user.click(screen.getByRole("button", { name: "转换所选图片" }));
    await waitFor(() => expect(calls.some(({ command }) => command === "convert_images")).toBe(true));
    await user.click(screen.getByRole("button", { name: "取消转换" }));
    expect(calls.some(({ command }) => command === "cancel_conversion")).toBe(true);
    expect(screen.getByRole("button", { name: "打开文件" })).toBeDisabled();
    await act(async () => { pending.resolve({ ...success, cancelled: true }); });
    await waitFor(() => expect(screen.getByRole("button", { name: "打开文件" })).toBeEnabled());
    expect(screen.getByText("已取消：1 张已完成")).toBeInTheDocument();
    const row = screen.getByRole("button", { name: "预览 figures/beta.jpg" }).closest("tr")!;
    expect(within(row).getByText("已取消")).toBeInTheDocument();
  });

  it("shows individual failures and excludes failed conversions from replacement eligibility", async () => {
    handlers.convert_images = () => ({ images: [{ id: "a", pdfPath: null, bytes: null, cached: false, error: "无法解码图片" }], cancelled: false });
    const user = await openProject();
    await user.click(screen.getByRole("checkbox", { name: "选择 figures/alpha.png" }));
    await user.click(screen.getByRole("button", { name: "转换所选图片" }));
    await screen.findByText("转换完成：0 成功，1 失败");
    expect(screen.getByText("无法解码图片")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "预览替换" })).toBeDisabled();
  });

  it("requires reviewing the diff before applying, then supports undo", async () => {
    const user = await openProject();
    await convertFirst(user);
    await user.click(screen.getByRole("button", { name: "预览替换" }));
    const dialog = await screen.findByRole("dialog", { name: "确认引用替换" });
    expect(within(dialog).getByText("\\includegraphics{figures/alpha.png}")).toBeInTheDocument();
    expect(within(dialog).getByText("\\includegraphics{figures/alpha.pdf}")).toBeInTheDocument();
    expect(calls.some(({ command }) => command === "apply_replacements")).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "应用替换" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByText("已替换 1 个文件中的 1 处引用")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "选择 figures/alpha.png" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "预览替换" })).toBeDisabled();
    expect(calls.filter(({ command }) => command === "open_project")).toHaveLength(2);
    await waitFor(() => expect(screen.getByRole("button", { name: "撤销替换" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "撤销替换" }));
    await screen.findByText("已恢复 1 个文件中的 1 处引用");
    expect(calls.some(({ command }) => command === "undo_replacements")).toBe(true);
    expect(screen.getByRole("button", { name: "撤销替换" })).toBeDisabled();
    expect(calls.filter(({ command }) => command === "open_project")).toHaveLength(3);
  });

  it("dismisses the review with Escape without mutating files", async () => {
    const user = await openProject();
    await convertFirst(user);
    await user.click(screen.getByRole("button", { name: "预览替换" }));
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls.some(({ command }) => command === "apply_replacements")).toBe(false);
  });

  it("keeps an apply failure in the review and allows retry", async () => {
    handlers.apply_replacements = () => { throw new Error("文件已被修改，请重新扫描"); };
    const user = await openProject();
    await convertFirst(user);
    await user.click(screen.getByRole("button", { name: "预览替换" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "应用替换" }));
    await within(dialog).findByText("文件已被修改，请重新扫描");
    expect(within(dialog).getByRole("button", { name: "应用替换" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "撤销替换", hidden: true })).toBeDisabled();
  });

  it("rescans the current main file and clears stale selection and conversion state", async () => {
    const user = await openProject();
    await convertFirst(user);
    handlers.open_project = () => ({ ...structuredClone(project), images: [project.images[1]], warnings: [] });
    await user.click(screen.getByRole("button", { name: "重新扫描" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "预览 figures/alpha.png" })).not.toBeInTheDocument());
    expect(screen.getByRole("checkbox", { name: "选择 figures/beta.jpg" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "预览替换" })).toBeDisabled();
    expect(calls.filter(({ command }) => command === "open_project")).toHaveLength(2);
    expect(calls.filter(({ command }) => command === "plugin:dialog|open")).toHaveLength(1);
  });

  it("does not let a late original preview replace the newly selected image", async () => {
    const oldPreview = deferred<{ data: string; kind: string }>();
    handlers.asset_preview = ({ imageId }) => imageId === "a" ? oldPreview.promise : { data: secondImageData, kind: "image" };
    const user = await openProject();
    await waitFor(() => expect(calls.some(({ command, args }) => command === "asset_preview" && args.imageId === "a")).toBe(true));
    await user.click(screen.getByRole("button", { name: "预览 figures/beta.jpg" }));
    const image = await screen.findByRole("img", { name: "原始：figures/beta.jpg" });
    expect(image).toHaveAttribute("src", secondImageData);
    await act(async () => { oldPreview.resolve({ data: imageData, kind: "image" }); });
    expect(screen.getByRole("img", { name: "原始：figures/beta.jpg" })).toHaveAttribute("src", secondImageData);
    expect(screen.queryByRole("img", { name: "原始：figures/alpha.png" })).not.toBeInTheDocument();
  });

  it("requests the converted asset only after completion and the converted tab is chosen", async () => {
    const user = await openProject();
    expect(screen.getByRole("radio", { name: "转换后" })).toBeDisabled();
    await convertFirst(user);
    await user.click(screen.getByRole("radio", { name: "转换后" }));
    await screen.findByRole("img", { name: "转换后：figures/alpha.png" });
    expect(calls).toContainEqual({ command: "asset_preview", args: { imageId: "a", converted: true } });
  });
});
