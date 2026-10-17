import { copyFile, mkdir, readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const [target, platform, arch] = process.argv.slice(2);
const formats = {
  windows: [["nsis", ".exe"]],
  linux: [["deb", ".deb"], ["appimage", ".AppImage"]],
  macos: [["dmg", ".dmg"]],
}[platform];
if (!target || !formats || !["x64", "arm64"].includes(arch)) throw new Error("Expected target, platform and architecture");
const desktop = fileURLToPath(new URL("../", import.meta.url));
const { version } = JSON.parse(await readFile(path.join(desktop, "package.json"), "utf8"));
if (process.env.GITHUB_REF?.startsWith("refs/tags/desktop-v") && process.env.GITHUB_REF !== `refs/tags/desktop-v${version}`) {
  throw new Error(`Release tag must match desktop-v${version}`);
}
const destination = path.resolve(desktop, "../desktop-installers");
await mkdir(destination, { recursive: true });
for (const [folder, extension] of formats) {
  const source = path.join(desktop, "src-tauri/target", target, "release/bundle", folder);
  const files = (await readdir(source)).filter((file) => file.endsWith(extension));
  if (!files.length) throw new Error(`Missing ${platform} ${extension} installer`);
  for (const file of files) {
    if (!(await stat(path.join(source, file))).size) throw new Error(`Empty installer: ${file}`);
    const name = `texgo-desktop_${version}_${platform}_${arch}__${file}`;
    await copyFile(path.join(source, file), path.join(destination, name));
    console.log(name);
  }
}
