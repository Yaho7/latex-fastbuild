"""CI-only checks of the CLI shipped inside desktop installers, not build outputs."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import struct
import subprocess
import tempfile


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


class Verification:
    def __init__(self, target, version):
        self.target = target
        self.version = version
        self.commands = []
        self.packages = []
        self.host = self.run(["rustc", "--print", "host-tuple"]).strip()

    def run(self, command, *, cwd=None, input=None, timeout=120):
        command = command if isinstance(command, str) else [str(arg) for arg in command]
        print(f"Running: {command}", flush=True)
        record = {"command": command}
        self.commands.append(record)
        try:
            result = subprocess.run(
                command, cwd=cwd, input=input, text=True, encoding="utf-8",
                errors="replace", capture_output=True, timeout=timeout, check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            record["error"] = str(error)
            raise
        record.update({
            "returncode": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr,
        })
        require(result.returncode == 0, f"Command failed: {command}\n{result.stdout}\n{result.stderr}")
        return result.stdout

    def executable(self, path):
        require(path.is_file() and path.stat().st_size > 0, f"Missing packaged executable: {path}")
        with path.open("rb") as source:
            header = source.read(64)
            if "windows" in self.target:
                require(header[:2] == b"MZ" and len(header) == 64, f"Not a PE executable: {path}")
                source.seek(struct.unpack_from("<I", header, 60)[0])
                pe = source.read(6)
                require(pe == b"PE\0\0\x64\x86", f"Expected x64 PE executable: {path}")
            elif "linux" in self.target:
                require(header[:6] == b"\x7fELF\x02\x01", f"Not a little-endian ELF64 executable: {path}")
                require(struct.unpack_from("<H", header, 18)[0] == 62, f"Expected x64 ELF: {path}")
            else:
                require(header[:4] == b"\xcf\xfa\xed\xfe", f"Not a little-endian Mach-O64 executable: {path}")
                cpu = 0x0100000C if self.target.startswith("aarch64-") else 0x01000007
                require(struct.unpack_from("<I", header, 4)[0] == cpu, f"Wrong Mach-O CPU: {path}")
        if os.name != "nt":
            require(os.access(path, os.X_OK), f"Executable permission lost: {path}")
        with path.open("rb") as source:
            return hashlib.file_digest(source, "sha256").hexdigest()

    def check(self, engine, gui, record, cwd):
        record["engine"] = str(engine)
        record["engine_sha256"] = self.executable(engine)
        record["gui_sha256"] = self.executable(gui)
        if self.host != self.target:
            record["execution"] = f"skipped: target {self.target} does not match native host {self.host}"
            print(record["execution"], flush=True)
            return
        help_text = self.run([engine, "--help"], cwd=cwd)
        require("texgo-engine run --config" in help_text, "Packaged CLI help is invalid")
        request = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools.info"}) + "\n"
        response = json.loads(self.run([engine, "rpc"], cwd=cwd, input=request))
        require(response.get("jsonrpc") == "2.0" and response.get("id") == 1, "Invalid RPC response")
        info = response.get("result", {})
        require(info.get("name") == "texgo-engine", "Wrong packaged engine")
        require(info.get("version") == self.version, "Packaged engine version mismatch")
        require("pipeline.run" in info.get("methods", []), "Packaged engine lacks pipeline.run")
        record["execution"] = "passed: --help and tools.info from packaged binary"
        record["tools_info"] = info

    def windows(self, package, work, record):
        import winreg

        def registry_paths():
            values = []
            for hive, key in [
                (winreg.HKEY_CURRENT_USER, "Environment"),
                (winreg.HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"),
            ]:
                with winreg.OpenKey(hive, key) as handle:
                    try:
                        values.append(winreg.QueryValueEx(handle, "Path"))
                    except FileNotFoundError:
                        values.append(None)
            return values

        before = registry_paths()
        installed = work / "Installed Texgo"
        # NSIS requires /D= last and unquoted, including when the path has spaces.
        command = f'"{package}" /S /NS /D={installed}'
        try:
            self.run(command, timeout=600)
            require(registry_paths() == before, "Installer changed persistent PATH")
            require((installed / "uninstall.exe").is_file(), "Installer did not provide an uninstaller")
            self.check(installed / "texgo-engine.exe", installed / "texgo-desktop.exe", record, work)
            record["installation"] = "passed: silent current-user NSIS install to a path with spaces"
        finally:
            uninstaller = installed / "uninstall.exe"
            if uninstaller.is_file():
                # _?= keeps NSIS in this process, so completion is observable.
                self.run(f'"{uninstaller}" /S _?={installed}', timeout=120)
                require(not (installed / "texgo-engine.exe").exists(), "Uninstall left the engine behind")
                require(not (installed / "texgo-desktop.exe").exists(), "Uninstall left the GUI behind")
                record["uninstallation"] = "passed: GUI and CLI removed together"
            require(registry_paths() == before, "Install/uninstall changed persistent PATH")
            record["persistent_path"] = "unchanged: user and machine registry values"

    def macos(self, package, work, record):
        mount = work / "mounted"
        mount.mkdir()
        self.run(["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", mount, package])
        try:
            app = work / "Installed Apps" / "Texgo.app"
            self.run(["ditto", mount / "Texgo.app", app])
            with (app / "Contents/Info.plist").open("rb") as source:
                info = plistlib.load(source)
            require(info["CFBundleShortVersionString"] == self.version, "DMG app version mismatch")
            binaries = app / "Contents/MacOS"
            engine = binaries / "texgo-engine"
            self.check(engine, binaries / info["CFBundleExecutable"], record, work)
            self.run(["codesign", "--verify", "--strict", engine])
            self.run(["codesign", "--verify", "--strict", app])
            record["installation"] = "passed: copied app from read-only DMG; ad-hoc signatures verified"
        finally:
            self.run(["hdiutil", "detach", mount])

    def linux(self, package, work, record):
        if package.suffix == ".deb":
            root = work / "deb-root"
            self.run(["dpkg-deb", "--extract", package, root])
        else:
            self.run([package, "--appimage-extract"], cwd=work)
            root = work / "squashfs-root"
        self.check(root / "usr/bin/texgo-engine", root / "usr/bin/texgo-desktop", record, work)
        record["installation"] = "payload extracted; package manager/desktop integration not exercised"

    def verify(self):
        root = Path("desktop/src-tauri/target") / self.target / "release/bundle"
        if "windows" in self.target:
            formats, inspect = [("nsis", "*.exe")], self.windows
        elif "apple" in self.target:
            formats, inspect = [("dmg", "*.dmg")], self.macos
        else:
            formats, inspect = [("deb", "*.deb"), ("appimage", "*.AppImage")], self.linux
        for folder, pattern in formats:
            packages = sorted((root / folder).glob(pattern))
            require(packages, f"No actual packages found: {root / folder / pattern}")
            for package in packages:
                record = {"package": str(package), "status": "failed"}
                self.packages.append(record)
                try:
                    with tempfile.TemporaryDirectory(prefix="texgo package check ") as temporary:
                        inspect(package.resolve(), Path(temporary), record)
                except Exception as error:
                    record["error"] = str(error)
                    raise
                record["status"] = "passed"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    require(os.environ.get("GITHUB_ACTIONS") == "true", "Installer verification is GitHub Actions-only")
    verification = Verification(args.target, args.version)
    try:
        verification.verify()
    finally:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps({
            "target": args.target, "native_host": verification.host, "version": args.version,
            "packages": verification.packages, "commands": verification.commands,
        }, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
