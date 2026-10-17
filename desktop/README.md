# Texgo Desktop

Tauri 2 + Rust + React + shadcn/ui. The workbench scans images referenced by a
TeX main file, converts selected images to PDF, previews results and applies
reversible reference replacements. Source images remain intact.

Every installer includes the independent `texgo-engine` CLI/JSON-RPC API.
Install once for both GUI and automation; no second engine download is needed.
Use its [installed path](../docs/desktop-release.md#one-install-gui-and-cli)
in VS Code or another editor. Compilation needs an installed TeX distribution
and `latexmk`; image conversion does not need GraphicsMagick or TeX.

## Development

Local static check, with development dependencies installed:

```sh
npm run check
```

Compilation and packaging happen on GitHub Actions, not locally. Existing unit
tests and browser fixtures remain available for explicitly requested testing.

## Packaging

**Every push triggers the desktop build.** Four targets package immediately in
parallel: Windows x64, Linux x64, macOS Apple Silicon and Intel. Ordinary pushes
produce Actions artifacts. `desktop-v*` tag pushes also publish a GitHub Release
after all packages complete, including `SHA256SUMS`.

There are no cloud test, screenshot, integration or lockfile-transfer stages.
The engine is built before Tauri so it can be bundled into the same installer.

macOS requires 13.3+. Windows requires WebView2; Linux requires current WebKitGTK
4.1. Windows builds are unsigned; macOS builds are ad-hoc signed and not
notarized. See [packaging and installed paths](../docs/desktop-release.md),
[Chinese guide](README.zh.md), [API](../docs/api.md),
[schema](../docs/project.schema.json) and [editor example](../examples/vscode/).
The repository's [MIT license](../LICENSE) applies.
