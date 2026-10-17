# texgo Desktop

The desktop application, named `Texgo` in Tauri, lives independently of the Go
CLI: React/Vite in this directory, and Tauri 2 with the `texgo-desktop` Rust crate
in `src-tauri/`.
Its initial version is `0.2.0`. The repository's [MIT license](../LICENSE) applies.

## CI and Downloads

The [Desktop workflow](../.github/workflows/desktop.yml) runs on pushes to `main`
and `codex/**`, pull requests, manual dispatch, and `desktop-v*` tag pushes.
Only desktop tag **pushes** publish a GitHub Release. CLI `v*` tags are separate.
Branch, PR, and manual runs provide downloadable workflow artifacts instead.

| Platform | Rust target | Installer |
| --- | --- | --- |
| Windows x64 | `x86_64-pc-windows-msvc` | NSIS `.exe` |
| Linux x64 (Ubuntu 22.04 build baseline) | `x86_64-unknown-linux-gnu` | `.deb`, `.AppImage` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.dmg` |
| macOS Intel | `x86_64-apple-darwin` | `.dmg` |

Installers are named
`texgo-desktop_<version>_<platform>_<arch>__<original-installer-name>`.
Use `macos-arm64` for Apple Silicon and `macos-x64` for Intel. Releases include
`SHA256SUMS` for all installers; workflow artifacts expire, release assets do not
have the workflow's retention period.

Windows builds are unsigned and can trigger SmartScreen or enterprise-policy
blocks. macOS builds are ad-hoc signed only, not Developer ID signed or notarized;
Gatekeeper can block them. Checksums do not establish publisher identity. See
[the release guide](../docs/desktop-release.md) before distributing these builds.

## Checks Run on GitHub Runners

These commands describe CI, not a requirement to install or run tools locally:

```sh
# Working directory: desktop/ (frontend gate)
npm install  # Generates package-lock.json when initially absent; npm ci if present
npm run check
npm test
npm run build

# Working directory: repository root, on Linux, Windows and macOS
cargo test --manifest-path desktop/src-tauri/Cargo.toml --package texgo-desktop --no-default-features --locked
```

CI creates a missing `package-lock.json` remotely and uploads it as
`desktop-npm-lock` before installing browser QA tools. Each packaging job
downloads that exact lock into `desktop/` and runs `npm ci`, without resolving
frontend dependencies independently. If an npm lock is already committed, the
frontend gate also uses `npm ci` and preserves the committed lock.

CI similarly resolves a missing `src-tauri/Cargo.lock` and shares it as
`desktop-cargo-lock` across Rust test and packaging jobs. Both lock artifacts
last 14 days and can be reviewed and adopted into source control without a local
install/build. Until committed, new workflow runs may resolve newer versions;
within a run, jobs share the resolved application dependency graphs. Browser QA
tooling is installed separately and is not part of the application npm lock.

All four installer builds wait for frontend checks, browser preview QA, and all
three operating systems' core tests. Packaging enables the normal `desktop`
default feature through Tauri; core tests disable default features and include
`src-tauri/tests` without requiring a GUI. The real Tauri builds compile the
default-feature code, so no duplicate `cargo check` step is needed.

Every packaging job runs `npx tauri icon app-icon.svg` from `desktop/` after
`npm ci` and before building Tauri. It verifies that `src-tauri/icons/`
contains nonempty `icon.png`, `icon.ico`, and `icon.icns`, matching the bundle
configuration. These generated files stay on CI runners, not in source control.
Pure Rust core tests do not generate or require icons.

After packaging, Linux CI runs the generated `texgo-desktop` executable from an
empty temporary working directory under Xvfb for 10 seconds. It requires timeout
exit status `124`, failing on any early exit or crash. Logs are retained as
`desktop-native-startup-linux-x64`. This is only a native process-liveness check:
it does not validate GUI rendering, dialogs, installation, or native workflows.

## Browser QA Boundary

CI serves the production Vite build at `http://127.0.0.1:4173` and captures Chromium
screenshots at 1440x1000 and 390x844, uploaded as `desktop-browser-qa`. These are
browser previews of the real frontend, not screenshots of an installed Tauri app.
Rendering `#root > *` is a smoke check, not a visual-regression assertion.

After the generic screenshots, `node scripts/browser-qa.mjs` runs fixture-based
browser interaction checks with the already installed Playwright Chromium. The
native bridge, project state, conversion results, replacement and undo responses
are **mocked fixtures**, not real native operations. The image preview uses a
real PNG asset, and PDF.js renders a generated vector PDF fixture into a real
canvas whose pixels are checked. The script exercises conversion/review/undo
UI flows, checks for browser errors and horizontal overflow at 1024px and 390px,
and uploads explicitly named `fixture-*.png` screenshots in `desktop-browser-qa`.

An optional `test:e2e` npm script runs against that same preview with
`PLAYWRIGHT_BASE_URL=http://127.0.0.1:4173`. Tests/configuration can be supplied
separately; make their Playwright config read this variable, avoid starting a
second server, and use Chromium (the installed browser). Reports in
`playwright-report/` and `test-results/` are retained even on failure.

Further fixture coverage can include compiler logs, paths with spaces, missing
TeX installations and more failure states. Browser results do not prove native
dialogs, filesystem mutations, actual image conversion, TeX installation, real
compilation or external PDF opening.

See [Desktop Releases](../docs/desktop-release.md) for exact packaging commands,
version/tag rules, signing, release permissions, and failure recovery.
