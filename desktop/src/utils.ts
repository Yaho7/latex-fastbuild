export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function relativeFile(path: string, root: string): string {
  const normalizedPath = path.replaceAll("\\", "/");
  const prefix = `${root.replaceAll("\\", "/").replace(/\/$/, "")}/`;
  return normalizedPath.startsWith(prefix) ? normalizedPath.slice(prefix.length) : path;
}

export function formatBytes(bytes: number | null): string {
  if (bytes === null) return "未知大小";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function errorMessage(error: unknown, fallback = "操作失败，请重试"): string {
  const message = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  return message.trim().slice(0, 300) || fallback;
}
