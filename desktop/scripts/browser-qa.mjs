import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import { chromium } from "@playwright/test";

const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? "http://127.0.0.1:4173";
const logo = await readFile(new URL("../../cmd/texgo/template-assets/logo.png", import.meta.url));
const imageData = `data:image/png;base64,${logo.toString("base64")}`;
const reference = { file: "/论文/main.tex", line: 42, original: "figures/result.png", start: 10, end: 28 };
const project = {
  mainFile: "/论文/main.tex", root: "/论文", texFiles: ["/论文/main.tex", "/论文/chapters/results.tex"],
  undoAvailable: false, warnings: ["未找到 figures/missing.png"],
  images: [
    { id: "plot", path: "/论文/figures/result.png", relativePath: "figures/result.png", format: "png", width: 2400, height: 1600, bytes: 1000000, status: "ready", message: null, references: [reference] },
    { id: "long", path: "/论文/figures/实验装置与磁流体响应结果.png", relativePath: "figures/实验装置与磁流体响应结果.png", format: "png", width: 1200, height: 900, bytes: 300000, status: "ready", message: null, references: [{ ...reference, line: 60 }] },
    { id: "missing", path: null, relativePath: "figures/missing.png", format: "png", width: null, height: null, bytes: null, status: "missing", message: "找不到图片", references: [{ ...reference, line: 75 }] },
  ],
};

// A small vector PDF fixture exercises the real PDF.js canvas renderer.
function pdfFixture() {
  const drawing = "0.08 0.49 0.44 RG 4 w 30 30 m 90 75 l 150 50 l 210 120 l 270 135 l S\n";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 160] /Resources << >> /Contents 4 0 R >>",
    `<< /Length ${Buffer.byteLength(drawing)} >>\nstream\n${drawing}endstream`,
  ];
  let pdf = "%PDF-1.4\n";
  const offsets = [0];
  objects.forEach((object, i) => { offsets.push(Buffer.byteLength(pdf)); pdf += `${i + 1} 0 obj\n${object}\nendobj\n`; });
  const start = Buffer.byteLength(pdf);
  pdf += `xref\n0 5\n0000000000 65535 f \n${offsets.slice(1).map((offset) => `${String(offset).padStart(10, "0")} 00000 n \n`).join("")}trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n${start}\n%%EOF\n`;
  return Buffer.from(pdf).toString("base64");
}

await mkdir("screenshots", { recursive: true });
const browser = await chromium.launch();
try {
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(({ project, imageData, pdf }) => {
    const callbacks = new Map();
    let sequence = 0;
    let applied = false;
    window.__TAURI_INTERNALS__ = {
      transformCallback(callback) { const id = ++sequence; callbacks.set(id, callback); return id; },
      unregisterCallback(id) { callbacks.delete(id); },
      async invoke(command, args = {}) {
        if (command === "plugin:event|listen") return ++sequence;
        if (command === "plugin:event|unlisten") return;
        if (command === "plugin:dialog|open") return project.mainFile;
        if (command === "open_project") return { ...project, undoAvailable: applied };
        if (command === "asset_preview") return args.converted ? { data: pdf, kind: "pdf" } : { data: imageData, kind: "image" };
        if (command === "convert_images") return { images: args.options.imageIds.map((id) => ({ id, pdfPath: `/论文/figures/texgo-pdf/${id}.pdf`, bytes: 5000, cached: false, error: null })), cancelled: false };
        if (command === "preview_changes") return { files: [{ path: project.mainFile, changes: [{ line: 42, before: "figures/result.png", after: "figures/texgo-pdf/plot.pdf" }] }], referenceCount: 1 };
        if (command === "apply_replacements") { applied = true; return { filesChanged: 1, referencesChanged: 1 }; }
        if (command === "undo_replacements") { applied = false; return { filesChanged: 1, referencesChanged: 1 }; }
        throw new Error(`Unexpected fixture command: ${command}`);
      },
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  }, { project, imageData, pdf: pdfFixture() });
  await page.goto(baseURL);
  await page.getByRole("button", { name: "打开主文件", exact: true }).click();
  await page.getByRole("button", { name: "预览 figures/result.png", exact: true }).waitFor();
  await page.getByRole("img", { name: "原始：figures/result.png", exact: true }).waitFor();
  await page.screenshot({ path: "screenshots/fixture-workbench-desktop.png", fullPage: true });
  await page.getByRole("checkbox", { name: "选择 figures/result.png", exact: true }).check();
  await page.getByRole("button", { name: "转换所选图片", exact: true }).click();
  await page.getByRole("radio", { name: "转换后", exact: true }).click();
  await page.locator("canvas").waitFor();
  await page.waitForFunction(() => {
    const canvas = document.querySelector("canvas");
    if (!canvas || canvas.width === 0) return false;
    const pixels = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
    for (let i = 0; i < pixels.length; i += 4) if (pixels[i + 3] && pixels[i + 1] > pixels[i] + 20) return true;
    return false;
  });
  await page.screenshot({ path: "screenshots/fixture-converted-pdf.png", fullPage: true });
  const fitted = await page.locator(".pdf-viewport").evaluate((viewport) => {
    const canvas = viewport.querySelector("canvas");
    if (!canvas) return false;
    const page = canvas.getBoundingClientRect();
    return page.height + 32 <= viewport.clientHeight + 1 && page.width + 48 <= viewport.clientWidth + 1;
  });
  assert.equal(fitted, true, "Default PDF preview must fit the whole page without cropping");
  await page.getByRole("button", { name: "预览替换", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "确认引用替换" });
  await dialog.waitFor();
  await page.screenshot({ path: "screenshots/fixture-replacement-review.png", fullPage: true });
  await dialog.getByRole("button", { name: "应用替换", exact: true }).click();
  await dialog.waitFor({ state: "hidden" });
  await page.getByRole("button", { name: "撤销替换", exact: true }).click();
  for (const width of [1024, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: `screenshots/fixture-workbench-${width}.png`, fullPage: true });
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 1);
    assert.equal(overflow, false, `Workbench overflows at ${width}px`);
  }
  assert.deepEqual(errors, [], "Frontend raised browser errors");
  await context.close();
} finally {
  await browser.close();
}
