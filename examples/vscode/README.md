# VS Code Integration

Open this directory as the VS Code workspace. Put the released `texgo-engine`
binary and your TeX distribution's `latexmk`/`xelatex` on `PATH`.

Run **Tasks: Run Build Task** to execute the named default task **Texgo: build**,
or **Tasks: Run Task → Texgo: watch** for a persistent background build. Stop it
with **Tasks: Terminate Task**. No task starts on folder open.

The optional `.vscode/settings.json` also connects LaTeX Workshop to the same
Texgo recipe, enables build-on-save, and points its PDF viewer at `build/`.
The extension is recommended by `.vscode/extensions.json`. When using this
build-on-save route, leave the separate watch task stopped to avoid duplicate
build controllers. Keep Workshop's `latex.outDir` aligned with `build.outputDir`
if you customize the project configuration. Build magic comments are disabled
so `TeX program` comments cannot bypass the Texgo recipe; configure the engine
in `.texgo/project.json` instead.

The configuration follows the extension's official
[compile recipe documentation](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)
and [PDF viewer documentation](https://github.com/James-Yu/LaTeX-Workshop/wiki/View).

`.texgo/project.json` resolves `../main.tex` against the config directory and
`build` against the main document directory. The example explicitly sets
`images.replace:true`: the initial build converts the SVG and replaces its TeX
path, while preserving the source SVG and a persisted undo transaction.
Subsequent watch runs detect changes to the original SVG. Set `replace:false`
to disable reference edits; SVG references then require your own TeX-side SVG
support when compiling unchanged sources.

The config's `$schema` points to the schema in this repository. After copying
the example elsewhere, update or remove that optional hint. Release archives
include `project.schema.json` at their root; point to its location relative to
your config (for example `../project.schema.json` if placed beside `.texgo`).

Results go to `build/main.pdf`; full captured compiler output goes to
`build/main.texgo-output.log` independently of TeX's native `build/main.log`.
Task errors and warnings appear in VS Code's Problems panel. Workshop uses its
own log/output handling; its GUI integration has not been exercised by the
headless CI fixture. See `docs/api.md` for
the CLI, JSON result contracts, and stateful JSON-RPC integration.
