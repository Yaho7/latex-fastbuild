# Headless Developer API

`texgo-engine` is the standalone, local Rust engine. It does not launch the
desktop UI. Integrate it as a process in VS Code, another editor, a script, or
an agent. The API is newline-delimited JSON-RPC 2.0 over stdin/stdout, not HTTP.
No listener, account, token, or cloud service is involved.

Installing Texgo Desktop also installs this engine. See the
[bundled executable paths](desktop-release.md#one-install-gui-and-cli) and use
that absolute path as the editor's process command. No second engine archive
or installation is required.
Compilation requires an existing TeX distribution providing `latexmk` and the
chosen engine. Texgo never downloads or installs TeX. Run only trusted TeX
projects: latexmk, TeX, project latexmkrc files, packages, and external tools
execute with the current user's permissions. Output-directory checks are not
a sandbox for arbitrary TeX code.

For source builds in CI, the headless target is:

```sh
cargo build --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features automation --bin texgo-engine
cargo test --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features automation
```

## Commands

```sh
texgo-engine scan --main /project/main.tex --json
texgo-engine build --config /project/.texgo/project.json --json
texgo-engine run --config /project/.texgo/project.json --json
texgo-engine watch --config /project/.texgo/project.json
texgo-engine rpc
```

`--json` is optional for `scan`, `build`, `run`, and `watch`. JSON commands never
print prose or compiler output to stdout. One-shot commands emit one JSON value
and exit 0 on success, 1 on failure. `scan` succeeds when scanning completes;
inspect asset statuses and warnings before deciding whether to proceed.

| Command | Behavior |
| --- | --- |
| `scan --main PATH` | Scan reachable TeX sources and image references without editing them. |
| `build --config PATH` | Compile only; never convert images or replace TeX references. |
| `run --config PATH` | Scan, validate, optionally convert/replace, then compile. |
| `watch --config PATH` | Run the pipeline immediately, then repeat after relevant edits. |
| `rpc` | Keep a Workspace session alive and accept one JSON-RPC object per line. |

Command-line paths are relative to the process working directory. Paths in the
configuration follow the separate rules below. A one-shot configuration or
pipeline preflight error has this shape and a nonzero process exit:

```json
{"error":{"message":"Pipeline cannot resolve all image references: missing.png (missing)"}}
```

A failed compile is **not** replaced by this generic envelope: `build` retains
the entire `BuildResult`, and `run` retains its `PipelineResult` with
`build.success:false`, while still exiting nonzero.

## Version 1 Configuration

For `/project/.texgo/project.json`:

```json
{
  "version": 1,
  "mainFile": "../main.tex",
  "images": {
    "enabled": true,
    "quality": "original",
    "dpi": 300,
    "maxWidthMm": 160,
    "jpegQuality": 90,
    "replace": true
  },
  "build": {
    "engine": "xelatex",
    "outputDir": "build",
    "synctex": true
  }
}
```

`version` and `mainFile` are required. Unknown properties, wrong JSON types,
unsupported versions, and invalid option ranges are rejected, including in
nested objects. Optional `$schema` is an editor hint, not a schema URL fetched
by the engine. The schema is maintained at `docs/project.schema.json` in the
repository and tagged release source.

| Property | Default | Contract |
| --- | --- | --- |
| `version` | Required | Exactly `1`. |
| `mainFile` | Required | Existing UTF-8 `.tex` file; relative to the **config's parent directory**, or absolute. |
| `images.enabled` | `true` | Convert every ready image when running the pipeline. |
| `images.quality` | `"original"` | `original`, `high`, `compact`, or `custom`. |
| `images.dpi` | `300` | Integer 72 through 1200. |
| `images.maxWidthMm` | `160` | Finite number 10 through 1000. |
| `images.jpegQuality` | `90` | Integer 1 through 100. |
| `images.replace` | `false` | Only explicit JSON `true` authorizes automated reference edits. |
| `build.engine` | `"xelatex"` | `xelatex`, `lualatex`, or `pdflatex`. |
| `build.outputDir` | `"build"` | Dedicated strict subdirectory of the main file's canonical parent. |
| `build.synctex` | `true` | Enable or disable SyncTeX. |

Thus `mainFile:"../main.tex"` and `outputDir:"build"` above compile
`/project/main.tex` into `/project/build`, not `/project/.texgo/build`.
The output directory may be absolute only if it remains inside the project.
It cannot be the project root, use `..`, contain symlinks, use reserved
`.git`/`.texgo`/`node_modules`/`texgo-pdf` directories, or contain referenced
project sources or other TeX/raster/vector source material.

`original` preserves original image quality and natural physical dimensions.
Other modes use the existing converter's sizing/compression policies. PDF
images already usable by TeX are not rasterized or reconverted.

### Pipeline Safety

`run`, `watch`, and RPC `pipeline.run` share the same pipeline:

1. Open the project and reject scan warnings and missing, ambiguous,
   unsupported, or otherwise unresolved references. These checks also apply
   when image conversion is disabled. Use `build` for intentional compile-only
   behavior on a project that cannot be statically resolved.
2. If enabled, convert **all** ready images. Any conversion error or cancellation
   stops the pipeline before any reference edits or compilation.
3. Only if `images.replace` is explicitly `true`, preview the replacement plan
   and apply it when `referenceCount > 0`. No UI prompt is needed: the config is
   the authorization. With omitted/false `replace`, conversion can populate the
   cache, but compilation still uses the unchanged original TeX references.
4. Invoke latexmk. A compiler failure does not automatically undo a successful
   reference replacement; its persisted undo record remains available.
   When a generated image or reference changes, pass `-g` so same-second,
   same-size path edits cannot leave a stale document PDF. Unchanged cache-hit
   runs retain latexmk's normal incremental behavior; auxiliary files are not
   deleted.

Source images are preserved. Conversion writes PDFs into `texgo-pdf` beside
the images. Managed reference provenance and the latest undo transaction live
under `.texgo`. On later scans the core resolves managed PDFs back to their
original images, so changed originals can regenerate PDFs. Cache hits with no
path changes skip `apply`, preserving the existing undo journal. A project
containing only existing PDF references can compile without any replacement.

The engine uses the core project's write lock around reference mutations and
around actual compilation/log persistence, so another managed writer cannot
modify that project during a compile. Concurrent lock contention is an error,
not an indefinite wait. This is not a lock against external editor processes;
core preflight and undo checks refuse to overwrite changed source files.
Undo restores references, not images or build artifacts. It is a one-level
persisted transaction, not a general editor history.

## JSON Result Contracts

All object field names use camelCase. Paths returned by the engine are absolute
unless a field explicitly describes a source-relative string. Nullable fields
are present as JSON `null`, not omitted.

### BuildResult

```json
{
  "success": false,
  "exitCode": 1,
  "pdfPath": null,
  "logPath": "/project/build/main.texgo-output.log",
  "diagnostics": [
    {"file":"/project/main.tex","line":12,"message":"Undefined control sequence.","severity":"error"}
  ]
}
```

`exitCode` is the native compiler status, or `null` if spawning failed or the
process had no numeric exit code. `pdfPath` is non-null only on a successful
compiler exit with an existing PDF. Never use an old PDF as evidence of success
after a failed compile. Each diagnostic has `file:string|null`,
`line:integer|null`, `message:string`, and `severity:"error"|"warning"`.

The full captured stdout and stderr are always persisted for an attempted
compiler invocation, even when latexmk is missing or exits unsuccessfully.
`<stem>.texgo-output.log` is separate from latexmk/TeX's native `<stem>.log`.
The capture appends stderr after stdout; byte content is retained, but stream
interleaving is not preserved. The latest run replaces the captured log.
Failure to persist that log is itself a reported operation error.

Diagnostic parsing recognizes `file.tex:N:`, `.sty`/`.cls`/`.bib` locations,
TeX `!` errors followed by `l.N`, warnings, overfull/underfull boxes, and latexmk
failures. Diagnostics are best effort; consult `logPath` for the full output.

The actual process invocation uses argv, not a shell:

```text
latexmk <engine-flag> -synctex=1 -file-line-error -interaction=nonstopmode -halt-on-error -outdir=<absolute-output-dir> main.tex
```

Engine flags are `-xelatex`, `-lualatex`, and `-pdf` respectively. SyncTeX off
uses `-synctex=0`. The working directory is the main file's parent and the final
argument is its basename. No untrusted string is interpolated into a shell
command. User/system latexmk configuration still applies.

### Project And Images

`scan`, `project.open`, and `project.scan` return:

```text
Project {
  mainFile: string, root: string, texFiles: string[],
  images: ImageAsset[], warnings: string[], undoAvailable: boolean
}
ImageAsset {
  id: string, path: string|null, relativePath: string, format: string,
  width: integer|null, height: integer|null, bytes: integer|null,
  status: string, message: string|null, references: Reference[]
}
Reference { file: string, line: integer, original: string, start: integer, end: integer }
```

Use opaque image IDs from the current scan. Status `ready` means convertible,
`pdf` means already usable, and other statuses such as `missing`, `ambiguous`,
and `unsupported` require attention. `line` is one-based; `start` and `end` are
UTF-8 byte offsets within the scanned source. `original` is the current path
text in TeX, which can already be a managed PDF path.

### Conversion, References, And Pipeline

```text
ConversionResult { images: ConvertedImage[], cancelled: boolean }
ConvertedImage {
  id: string, pdfPath: string|null, bytes: integer|null,
  cached: boolean, error: string|null
}
ReplacementPlan { files: FileChanges[], referenceCount: integer }
FileChanges { path: string, changes: ReferenceChange[] }
ReferenceChange { line: integer, before: string, after: string }
MutationResult { filesChanged: integer, referencesChanged: integer }
PipelineResult {
  project: Project, conversion: ConversionResult|null,
  replacement: MutationResult|null, build: BuildResult
}
```

`images.convert` may return individual conversion errors in a successful RPC
result. Inspect every `error` before applying. In contrast, the automated
pipeline treats any such error as failure and never compiles or applies a
partial batch. `conversion:null` means no conversion was needed/enabled;
`replacement:null` means no reference edits were applied.

## JSON-RPC 2.0 Stdio

Spawn `texgo-engine rpc`, write UTF-8 JSON objects ending in a newline to stdin,
and read one JSON response per request from stdout. Flush after writing. EOF
ends the session. There are no HTTP endpoints, batch arrays, unsolicited
progress events, or stdout banners.

Each request has `jsonrpc:"2.0"`, `method:string`, optional `params:object`, and
optional `id:string|number|null`. Omitted `params` means `{}`. Omitted `id`
means a notification: the operation is processed, but **no response** is
written, including on method/parameter/runtime failure. Explicit `id:null` is
still a request and receives a response. A malformed envelope is not a valid
notification and receives an error with a null ID where no valid ID exists.

| Method | Params | Result |
| --- | --- | --- |
| `tools.info` | `{}` | Engine version, config version, transport, methods, engines, compiler, request bound. |
| `project.open` | `{"mainFile":"/project/main.tex"}` | `Project`; select a project and clear previous session conversions. |
| `project.scan` | `{"mainFile":"/project/main.tex"}` or `{}` | Rescan/select project; clear previous session conversions. Omitted main uses the open project. |
| `images.convert` | `{"options":{"imageIds":["ID"],"quality":"original","dpi":300,"maxWidthMm":160,"jpegQuality":90}}` | `ConversionResult`; retain conversions in session. All option fields are required. |
| `references.preview` | `{}` | `ReplacementPlan` for successful session conversions; no writes. |
| `references.apply` | `{"confirm":true}` | `MutationResult`; apply the current plan and refresh session. |
| `references.undo` | `{"confirm":true}` | `MutationResult`; restore latest persisted transaction and refresh session. |
| `document.build` | `{"configPath":"/project/.texgo/project.json"}` | `BuildResult`; compile only, no reference mutation. |
| `pipeline.run` | `{"configPath":"/project/.texgo/project.json"}` | `PipelineResult`; config's explicit `images.replace:true` authorizes edits. |

Methods needing a Workspace fail until a project is opened. A failed open
preserves the previously open session. `project.scan` intentionally resets
conversion plans: use open → convert → preview → apply without rescanning in
between. Build/pipeline configuration paths are independent of the current
session project; a pipeline refreshes a matching session to discard stale
plans. Use `project.open` to inspect another project after a pipeline call.
RPC paths in params are relative to the server process cwd or absolute;
config-contained paths follow the config rules above.

Both reference mutation methods reject omitted, false, and non-boolean
confirmation. Conversion writes PDF cache files but does not edit TeX.
`pipeline.run` is also a potentially source-mutating operation: only pass a
trusted configuration with `images.replace:true` when that is intended.

Example request/response exchange (replace `ID` with an ID from the open result):

```json
{"jsonrpc":"2.0","id":1,"method":"project.open","params":{"mainFile":"/project/main.tex"}}
{"jsonrpc":"2.0","id":2,"method":"images.convert","params":{"options":{"imageIds":["ID"],"quality":"original","dpi":300,"maxWidthMm":160,"jpegQuality":90}}}
{"jsonrpc":"2.0","id":3,"method":"references.preview"}
{"jsonrpc":"2.0","id":4,"method":"references.apply","params":{"confirm":true}}
{"jsonrpc":"2.0","id":5,"method":"document.build","params":{"configPath":"/project/.texgo/project.json"}}
{"jsonrpc":"2.0","id":6,"method":"references.undo","params":{"confirm":true}}
```

Success responses use `{"jsonrpc":"2.0","id":1,"result":...}`. Compiler
failure remains a result with `success:false`/`build.success:false`; it does not
terminate the RPC process. Operation errors use:

```json
{"jsonrpc":"2.0","id":4,"error":{"code":-32602,"message":"Reference mutation requires confirm:true"}}
```

| Code | Meaning |
| --- | --- |
| `-32700` | Malformed JSON or invalid UTF-8. |
| `-32600` | Invalid JSON-RPC envelope, batch request, invalid ID, or oversized frame. |
| `-32601` | Unknown method. |
| `-32602` | Invalid/missing params, options, config, or mutation confirmation. |
| `-32000` | Project, conversion, filesystem, lock, or other operation failure. |

Requests and config files are limited to 1 MiB. Oversized input lines are drained
without retaining the full frame. Invalid requests do not kill the session;
subsequent newline-delimited requests are still processed. Broken input/output
pipes and other transport failures terminate the process.

## Watch And VS Code

`watch` runs once immediately, then polls every 250 ms and waits until changes
have been quiet for 500 ms before rebuilding. It watches additions, deletions,
and timestamp/size changes to `.tex`, `.bib`, `.sty`, `.cls`, raster images,
SVG/EPS/PS, and PDFs below the main file's directory, plus the config itself.
It skips the configured output directory, `.git`, `.texgo`, `node_modules`,
and `texgo-pdf`; the config is an explicit exception to `.texgo` exclusion.
Directory/file symlinks are not traversed. Traversal is bounded to 50,000
entries and 64 directory levels. Initial invalid configuration and fatal watch
filesystem errors exit nonzero; compiler/pipeline failures stay alive. A later
invalid/deleted config is reported and retried when it changes again.

Snapshots are taken **before** builds. Edits during compilation trigger the
next debounced build. Reference edits from the pipeline can cause one extra
self-induced build; no-op replacements and output exclusions prevent an
infinite rebuild loop. Stop watch using the editor's terminate-task command
or Ctrl+C. Multiple overlapping watchers for one project are not recommended.

Plain output has `[texgo] build-start` and `[texgo] watch-ready` lines for VS
Code's background matcher, including after failed builds. Diagnostic lines
are `file:line: error|warning: message`.

`watch --json` emits NDJSON events, flushing each line:

```json
{"event":"build-start"}
{"event":"build-result","result":{"project":{},"conversion":null,"replacement":null,"build":{"success":true,"exitCode":0,"pdfPath":"/project/build/main.pdf","logPath":"/project/build/main.texgo-output.log","diagnostics":[]}}}
{"event":"watch-ready"}
```

The `project:{}` above is abbreviated for readability; real events contain a
complete `Project`. A preflight/config failure emits
`{"event":"build-result","error":{"message":"..."}}` instead. Build
failures retain the full pipeline result and log path. `watch-ready` means the
watcher is idle, not that the previous build succeeded.

The complete manual-start example is in `examples/vscode`, including a sample
TeX document, SVG, config, and `.vscode/tasks.json`. Its default build task runs
the pipeline once; its background task runs watch. Neither runs automatically
on folder open. The example deliberately opts into reference replacement;
set `images.replace:false` to keep TeX references untouched.
