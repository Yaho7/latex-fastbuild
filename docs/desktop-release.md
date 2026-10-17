# Desktop Packaging

## Workflow

Every push triggers `.github/workflows/desktop.yml`, on every branch and tag.
Manual dispatch is also available. A newer push cancels the older run on the
same branch; tag runs are not cancelled automatically. Pull requests do not
start a second duplicate run.

There are only two jobs:

1. **Package**: four independent targets build immediately in parallel.
2. **Publish Release**: only `desktop-v*` tag pushes publish, after all four
   package targets succeed. Branch/manual runs keep installers as artifacts.

Actions installs locked dependencies, builds `texgo-engine`, stages it as a
Tauri sidecar, generates icons, and invokes the official Tauri build action.
It does not run frontend checks, unit tests, browser screenshots, TeX fixtures,
installer smoke tests or dependency-lock transfer jobs. Lockfiles are committed.
No separate engine archives are produced: every desktop installer includes it.

| Platform | Target | Installer |
| --- | --- | --- |
| Windows x64 | `x86_64-pc-windows-msvc` | NSIS `.exe` |
| Linux x64 | `x86_64-unknown-linux-gnu` | `.deb`, `.AppImage` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.dmg` |
| macOS Intel | `x86_64-apple-darwin` | `.dmg` |

Linux bundles use Ubuntu 22.04 as the build baseline. macOS requires 13.3+ for
the modern WebKit used by the shadcn/Tailwind UI. Windows requires WebView2;
Linux requires an up-to-date WebKitGTK 4.1. Bundling is not a claim that every
native GUI workflow has been tested on every operating system.

## Local Static Checks

Run static checks locally; do not compile or package the application locally.
With development dependencies already installed:

```sh
cd desktop
npm run check
node --check scripts/collect-installers.mjs
git diff --check
```

Also validate edited JSON/TOML/YAML with available parsers. `npm run check` is
TypeScript's `tsc --noEmit`; `npm run build` only invokes Vite and is used by
Tauri packaging on Actions. Tests remain in source for explicit focused use,
but are not automatic packaging gates. Dependency installation is a separate
local setup step and must respect the user's environment permissions.

## Release

Keep the versions in `desktop/package.json`, its lockfile, Cargo manifest and
lockfile, and `tauri.conf.json` aligned. After local static checks, push commits
normally to obtain installers. Push a new matching tag to publish:

```sh
git tag desktop-v0.2.1
git push origin desktop-v0.2.1
```

Do not move existing tags. The cancelled `desktop-v0.2.0` run did not publish
a release; the simplified workflow starts with `desktop-v0.2.1`.

The release job merges the four installer artifacts, generates `SHA256SUMS`,
uploads everything into a draft and then makes it public. It does not replace
existing releases or assets. Desktop releases use `--latest=false` to preserve
the legacy Go CLI's latest-release selection. Legacy `release.yml` runs only
for CLI `v*` tags or manual dispatch, not ordinary branch pushes.

## One Install, GUI and CLI

Installing the desktop package supplies both the GUI and the independent
`texgo-engine` CLI/JSON-RPC executable. The GUI need not be running to call it.
Document compilation still requires a local TeX distribution and `latexmk`.

| Format | Installed engine |
| --- | --- |
| macOS DMG | `/Applications/Texgo.app/Contents/MacOS/texgo-engine` |
| Windows NSIS | `<chosen install directory>\texgo-engine.exe`; default `%LOCALAPPDATA%\Texgo\texgo-engine.exe` |
| Debian | `/usr/bin/texgo-engine` |
| AppImage | `squashfs-root/usr/bin/texgo-engine` after `--appimage-extract` |

No shell profiles or persistent PATH settings are modified. Debian naturally
installs the command on PATH. On other platforms use the absolute engine path
in both `.vscode/tasks.json` and `.vscode/settings.json`; keep arguments as a
separate JSON array. Do not embed shell quotes, `~`, or `%LOCALAPPDATA%` in the
JSON command expecting shell expansion. Keep extracted AppImages in a stable
directory rather than using their temporary mount paths.

See [the API](api.md), [schema](project.schema.json) and
[VS Code example](../examples/vscode/). Tauri's
[sidecar mechanism](https://v2.tauri.app/develop/sidecar/) bundles the engine;
no webview shell permission or HTTP API listener is introduced.

## Integrity and Signing

Release filenames include version, OS and architecture. Compare the SHA-256
of the download against its matching line in `SHA256SUMS`. Windows packages
are unsigned. macOS packages are ad-hoc signed, not Developer ID signed or
notarized; security policies may warn or block them. Do not disable system-wide
protections. Trusted signing requires a separately configured certificate and
notarization setup. There is no automatic updater or updater signing feed.
