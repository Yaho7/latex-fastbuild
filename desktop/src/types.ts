export interface Reference {
  file: string;
  line: number;
  original: string;
  start: number;
  end: number;
}

export type AssetStatus = "ready" | "missing" | "unsupported" | "ambiguous" | "pdf";

export interface ImageAsset {
  id: string;
  path: string | null;
  relativePath: string;
  format: string;
  width: number | null;
  height: number | null;
  bytes: number | null;
  status: AssetStatus;
  message: string | null;
  references: Reference[];
}

export interface Project {
  mainFile: string;
  root: string;
  texFiles: string[];
  images: ImageAsset[];
  warnings: string[];
  undoAvailable: boolean;
}

export type Quality = "original" | "high" | "compact" | "custom";

export interface ConversionOptions {
  imageIds: string[];
  quality: Quality;
  dpi: number;
  maxWidthMm: number;
  jpegQuality: number;
}

export interface ConvertedImage {
  id: string;
  pdfPath: string | null;
  bytes: number | null;
  cached: boolean;
  error: string | null;
}

export interface ConversionResult {
  images: ConvertedImage[];
  cancelled: boolean;
}

export interface AssetPreviewData {
  data: string;
  kind: "image" | "pdf";
}

export interface ChangePreview {
  files: { path: string; changes: { line: number; before: string; after: string }[] }[];
  referenceCount: number;
}

export interface ReplacementResult {
  filesChanged: number;
  referencesChanged: number;
}

export interface ConversionProgress {
  current: number;
  total: number;
  name: string;
  status: string;
}

export interface ImageResult extends ConvertedImage {
  cancelled?: boolean;
}
