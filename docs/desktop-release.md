# Desktop Releases

## Scope and Triggers

`.github/workflows/desktop.yml` owns desktop CI and distribution. The existing
CLI workflow `.github/workflows/release.yml` is unchanged. Their tag namespaces
are independent: desktop uses `desktop-v*`, CLI uses `v*`.

| Event | Checks and all four package builds | Publish Release |
| --- | --- | --- |
| Push to `main` or `codex/**` | Yes | No |
| Pull request | Yes | No |
| `workflow_dispatch` on a selected ref | Yes | No, even when selecting a tag |
| Push `desktop-v<version>` tag | Yes | After every gate succeeds |
| Push CLI `v*` tag | No desktop run | No desktop release |

There are no path filters, so required checks are not silently skipped. The
workflow defaults to `contents: read`; only the final release job receives
`contents: write`. PR jobs never receive signing credentials or release-write
permissions. Checkout does not persist credentials. Concurrency is isolated by
workflow and branch/tag/PR: newer branch/PR runs cancel older runs, while runs
for the same tag queue without cancellation. Distinct tags cannot overwrite
one another's assets.

## Version Contract

These three literal versions must be identical, including any prerelease suffix:

- `desktop/package.json`: `version`
- `desktop/src-tauri/Cargo.toml`: `[package].version`
- `desktop/src-tauri/tauri.conf.json`: `version`

Use SemVer without build metadata. The first intended stable desktop release is
`0.2.0`, tagged `desktop-v0.2.0`. A tag such as `desktop-v0.2.0-beta.1` is also
supported, but requires **all three** versions to be `0.2.0-beta.1`, not `0.2.0`.
CI automatically marks versions with a prerelease suffix as GitHub prereleases.
Both stable and prerelease desktop releases use `--latest=false`, preserving
the repository's CLI latest-release selection.

The maintainer/parent owns committing and pushing. After reviewing the changes
and a successful branch run, the release commands are:

```sh
# Run by the maintainer, not by the local implementation agent.
git tag desktop-v0.2.0 <reviewed-commit-sha>
git push origin desktop-v0.2.0
```

No release command installs, builds, or tests the desktop app on the maintainer's
machine. Do not move an already published tag to retry a release.

## Runner Pipeline

1. On Ubuntu 22.04, Python 3.11 reads JSON/TOML and validates all versions and
   any tag. Node 22 runs `npm install` when `desktop/package-lock.json` is absent,
   or `npm ci` if already committed, then `npm run check`, `npm test`, and
   `npm run build` in `desktop/`. No initial npm lockfile/cache is assumed. The
   resolved `package-lock.json` is uploaded as `desktop-npm-lock` before the
   separate browser-tool installation. All packaging jobs download that lock
   into `desktop/` and use `npm ci`, rather than resolve their own versions.
2. Stable Rust generates `desktop/src-tauri/Cargo.lock` only when absent; the
   `desktop-cargo-lock` artifact supplies that exact lockfile to later jobs.
   Tests and packages use `--locked`. The initial runner-generated application
   lockfiles are now committed. A committed lockfile is preserved instead of regenerated.
   Both `desktop-npm-lock` and `desktop-cargo-lock` are retained for 14 days;
   maintainers can download, review, and commit the files as
   `desktop/package-lock.json` and `desktop/src-tauri/Cargo.lock` without any
   local dependency install or build. Without committed locks, separate runs
   can resolve different versions, but consumers within each run share the
   same locks. Browser tooling remains separate from the application npm lock.
3. Browser QA installs `@playwright/test@1` without saving it, installs Chromium,
   and starts `npm exec -- vite preview --host 127.0.0.1 --port 4173 --strictPort`.
   CI captures wide and narrow screenshots after React renders, then runs
   `node scripts/browser-qa.mjs`, followed by `npm run test:e2e --if-present`.
   The script uses installed Playwright Chromium and an explicitly **mocked
   native bridge**: project/conversion/replacement/undo responses are fixtures,
   while a real PNG and a generated vector PDF exercise the actual image and
   PDF.js renderers. It pixel-checks the PDF canvas, exercises conversion,
   replacement review and undo UI flows, checks browser errors and horizontal
   overflow at widths 1024 and 390, and writes `screenshots/fixture-*.png`.
   All screenshot/report artifacts are browser-only evidence, not proof of
   real native conversion, filesystem edits, undo transactions or TeX jobs.
4. Ubuntu 22.04, Windows Server 2022, and macOS 14 each run:

   ```sh
   cargo test --manifest-path desktop/src-tauri/Cargo.toml --package texgo-desktop --no-default-features --features automation --locked
   ```

5. A separate Ubuntu integration job installs test-only `latexmk` and pdfLaTeX,
   runs the headless pipeline on PNG/SVG fixtures, compiles a real document with
   SyncTeX, verifies repeat runs do not rewrite references, and modifies an
   original SVG to verify regeneration after replacement. Proof is uploaded as
   `desktop-automation-integration`. The frontend gate also validates the JSON
   configuration schema and VS Code example. Only after these checks,
   frontend/browser QA and the entire Rust test matrix pass, the
   official [`tauri-apps/tauri-action@v1`](https://github.com/tauri-apps/tauri-action)
   builds the four packages. Each packaging job first runs the following in
   `desktop/`, after downloading `desktop-npm-lock` and running `npm ci`, and
   before any default-feature native build:

   ```sh
   npx tauri icon app-icon.svg
   ```

   The Tauri CLI generates icons from the committed SVG into `src-tauri/icons/`.
   CI requires nonempty `icon.png`, `icon.ico`, and `icon.icns` before continuing,
   matching `bundle.icon` in the Tauri config. Icon generation is CI-only; no
   generated icon files are committed or required by no-default-features core
   tests. This follows the official [icon command](https://v2.tauri.app/develop/icons/).

   The build action uses `projectPath: desktop` and `tauriScript: npx tauri`,
   equivalent to the following commands in `desktop/`:

   ```sh
   npx tauri build --target x86_64-pc-windows-msvc --bundles nsis -- --locked --bin texgo-desktop
   npx tauri build --target x86_64-unknown-linux-gnu --bundles deb,appimage -- --locked --bin texgo-desktop
   npx tauri build --target aarch64-apple-darwin --bundles dmg -- --locked --bin texgo-desktop
   npx tauri build --target x86_64-apple-darwin --bundles dmg -- --locked --bin texgo-desktop
   ```

   The `--` separator forwards Cargo's `--locked` flag through Tauri.
   These actual release builds compile the default `desktop` feature on every
   packaging target, so a separate default-feature `cargo check` is redundant.

   The Tauri configuration sets `productName` to `Texgo`, runs `npm run build`
   through `build.beforeBuildCommand`, and points `frontendDist` to `../dist`;
   the frontend gate also builds it independently. `desktop/package.json`
   includes the Tauri 2 CLI dependency. The collection step accepts Tauri's
   `Texgo` installer filenames and adds the independent `texgo-desktop` prefix.
   Both macOS targets build on macOS 14; the Intel build is a cross-build, not
   proof of execution on an Intel Mac.

   Linux installs WebKitGTK 4.1, AppIndicator, librsvg, patchelf, xdg-utils,
   libfuse2, Xvfb and xauth. Ubuntu 22.04 is the glibc baseline, not a promise that every older
   distribution works. `APPIMAGE_EXTRACT_AND_RUN=1` permits bundler AppImages to
   run on the hosted runner without mounting FUSE. Downloaded tools/binaries
   execute only in CI; nothing is installed or executed locally by this change.
   After the package build, Linux also starts the generated executable from an
   empty temporary working directory, using X11 and software rendering settings:

   ```sh
   # binary is the absolute path to:
   # desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/texgo-desktop
   xvfb-run -a timeout --signal=TERM --kill-after=5s 10s "$binary"
   ```

   Only timeout exit status `124` passes; early normal exit, crash, launch
   failure or forced-kill status fails packaging. The temporary working
   directory is removed afterward. Standard output/error and the exit status
   are retained for 14 days as `desktop-native-startup-linux-x64`, including on
   failure. This is a native process-liveness check only. It does not establish
   GUI rendering or interaction, installer correctness, real conversion, native
   review/undo functionality, TeX compilation, or Windows/macOS launch behavior.
6. Each target collects every installer of its requested formats from
   `desktop/src-tauri/target/<target>/release/bundle/`. Missing or empty files
   fail the job. Files receive a version/OS/architecture prefix and are uploaded
   as `desktop-bundles-windows-x64`, `desktop-bundles-linux-x64`,
   `desktop-bundles-macos-arm64`, and `desktop-bundles-macos-x64`. CI artifacts
   last 14 days. No Tauri release inputs or token are provided to the action.
   Each job also builds `texgo-engine` with `--no-default-features --features
   automation --bin texgo-engine --release --locked` for its target. That binary
   has no GUI runtime dependency and is archived with API documentation,
   project schema and editor examples as
   `texgo-engine_<version>_<platform>_<arch>.zip` (Windows) or `.tar.gz` (Unix).
7. For desktop tag pushes only, the final job downloads **all four** artifact
   directories without merging them. It rejects missing/unexpected platforms,
   unrecognized extensions, wrong version/platform prefixes, empty files and
   duplicate filenames. Required formats are Windows `.exe`, Linux `.deb` plus
   `.AppImage`, and one or more `.dmg` files for **each** macOS architecture.
   All collected installers are included, not just the first matching file.
   Every platform must also provide its standalone engine archive.
8. The final job calculates `SHA256SUMS`, runs `sha256sum --check SHA256SUMS`, and
   archives the complete set as `desktop-release-<version>` for 30 days. It then
   runs `gh release create <tag> release/* --verify-tag --draft --latest=false`
   with a title, notes and optional `--prerelease`, followed by
   `gh release edit <tag> --draft=false --latest=false`. The public release is
   exposed only after the complete upload succeeds. Existing releases are never
   updated or clobbered. No updater feed or update signatures are published.

The version and filename checks validate release routing, not executable CPU
headers or installer runtime behavior. Apart from the limited Linux startup
liveness check and the real Linux automation fixture, native installation and
GUI workflows still require separate platform QA. The automation fixture does
not establish every TeX distribution or editor integration works identically.

## Integrity and Installation

Download the installer for the correct architecture together with `SHA256SUMS`.
From a directory containing all release installers, verify with:

```sh
sha256sum --check SHA256SUMS       # Linux
shasum -a 256 --check SHA256SUMS  # macOS
```

When downloading only one installer, compare its SHA-256 against the matching
line instead; a full manifest check reports other, absent installers as missing.
On Windows, use PowerShell `Get-FileHash -Algorithm SHA256 <installer.exe>` and
compare the digest with that filename's manifest entry. Checksums protect
integrity against the downloaded manifest, not identity or independent trust.

Windows NSIS installers require a supported Windows/WebView2 environment. Linux
`.deb` installation uses the system package manager; AppImages need executable
permission and a compatible runtime. macOS DMGs are architecture-specific.
These outputs are not claimed to be locally installed or launch-verified.
The application and release source use the [MIT license](../LICENSE).

## Signing Status and Optional Follow-Up

No signing secrets are required. Windows packages are unsigned. macOS packages
set `APPLE_SIGNING_IDENTITY=-` for ad-hoc signing: they do **not** carry a trusted
Developer ID signature and are **not** notarized. SmartScreen, Gatekeeper, or
organization policies may warn or refuse installation. Do not recommend
disabling system-wide security protections or blindly removing quarantine.

Signing is intentionally not activated by merely adding repository secrets.
A reviewed, tag-only signing extension can later replace the ad-hoc identity:

- macOS: wire `APPLE_CERTIFICATE` (base64 `.p12`),
  `APPLE_CERTIFICATE_PASSWORD`, and `APPLE_SIGNING_IDENTITY` as secrets; add
  notarization using `APPLE_ID`, app-specific `APPLE_PASSWORD`, and
  `APPLE_TEAM_ID`, or the supported App Store Connect API credentials. Never
  supply these to fork/PR builds. Validate the full signed/notarized install
  path before changing the release notes' warnings.
- Windows: use a trusted signing provider and Tauri's signing configuration or
  `bundle.windows.signCommand`, with provider credentials held in secrets.
  No generic `WINDOWS_CERTIFICATE` environment variable is assumed to make
  Tauri sign automatically. Signing requires workflow/configuration changes;
  adding secrets alone does nothing. SmartScreen reputation is separate.
- Tauri updater signing is another, independent mechanism. No
  `TAURI_SIGNING_PRIVATE_KEY` or updater-signature requirement is enabled here.

Consult the official [macOS signing guide](https://v2.tauri.app/distribute/sign/macos/)
and [Windows signing guide](https://v2.tauri.app/distribute/sign/windows/).

## Failure Recovery

Any failed frontend, browser, Rust-test, packaging or completeness check prevents
the publish job. Fix the source and repeat branch CI before tagging a new
version. Matrix fail-fast is disabled to retain diagnostics from other targets.

If upload/publication fails after draft creation, a draft can remain; it is not
a public partial release. Inspect the run's complete release artifact, then have
a maintainer remove the incomplete draft **without deleting/moving its tag**
before rerunning the failed release job. Reruns deliberately fail against an
existing release rather than replace its assets. A published installer change
requires a new version/tag. Manual workflow dispatch is verification-only and
cannot repair or publish a release.
