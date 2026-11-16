#!/usr/bin/env python3
"""macOS release with NSH's updater: tests, a Developer ID
build with the Developer ID profile (iCloud Production), the app notarized and stapled, its update archive, a signed
DMG (notarized and stapled too) and, with an update source, the signed update manifest (`nsh update-manifest`).

Nothing is uploaded. Settings come from the untracked `local.toml` (environment variables override; see
docs/DEVELOPMENT.md). The manifest is written only when `update_url` and `update_key` are set: the source is the NSH
build's own.

Outputs in build/dist/: `Safay.app` (notarized and stapled), `Safay-<version>.dmg` (first installs),
`Safay-<version>-macos-<arch>.zip` (the stapled app), `Safay-<version>-macos-<arch>.tar.gz` (the update archive),
`latest.json` and `latest.json.sig` (with an update source; publish them next to the archive, or under
`download_url_prefix`), and `Safay-<version>.env` (version, build, hashes, what was done).
"""
from pathlib import Path
import hashlib
import json
import os
import plistlib
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import package  # noqa: E402

root = package.root


def run(*command: str, **kwargs) -> subprocess.CompletedProcess:
    print("+", " ".join(command), flush=True)
    return subprocess.run(command, check=True, **kwargs)


def changelog_section(version: str) -> str | None:
    """This version's notes from CHANGELOG.md (`## [<version>] - <date>` up to the next `## `)."""
    lines, found = [], False
    for line in (root / "CHANGELOG.md").read_text(encoding="utf-8").splitlines():
        if line.startswith("## "):
            if found:
                break
            found = line.removeprefix("## ").split(" ", 1)[0].strip("[]") == version
            continue
        if found:
            lines.append(line)
    return "\n".join(lines).strip() if found else None


def notarize(path: Path, profile: str) -> None:
    run("xcrun", "notarytool", "submit", str(path), "--keychain-profile", profile, "--wait")


def main() -> None:
    config = package.settings()
    metadata = json.loads((root / "nsh.json").read_text(encoding="utf-8"))
    version, build = metadata["version"], str(metadata["build"])
    identity, notary = config["identity"], config["notary_profile"]
    update_url, update_key = config["update_url"], config["update_key"]
    if not identity.startswith("Developer ID Application"):
        raise SystemExit("error: set local.toml [signing] identity to a Developer ID Application certificate")
    if not notary:
        raise SystemExit("error: set local.toml [release] notary_profile (xcrun notarytool store-credentials)")
    manifest = bool(update_url)
    if manifest:
        if not update_url.startswith("https://"):
            raise SystemExit("error: update_url must be https")
        if not package.UPDATE_PUBLIC_KEY:
            raise SystemExit(
                "error: no update-signing key yet: run `python3 scripts/nsh.py keygen <path>/safay-update`, set local.toml "
                "[release] update_key to <path>/safay-update.secret and scripts/package.py UPDATE_PUBLIC_KEY to the .pub content"
            )
        secret = Path(os.path.expanduser(update_key)) if update_key else None
        if secret is None or not secret.is_file():
            raise SystemExit("error: set local.toml [release] update_key to the update-signing secret (nsh keygen)")
        # A manifest signed with another key would make every installed copy refuse the update: check the pair first.
        public = secret.with_suffix(".pub")
        if not public.is_file() or public.read_text(encoding="utf-8").strip() != package.UPDATE_PUBLIC_KEY:
            raise SystemExit(f"error: {public} does not hold the public key in scripts/package.py")
        if changelog_section(version) is None:
            raise SystemExit(f"error: CHANGELOG.md has no '## {version}' section for the update notes")
    else:
        print("note: update_url is empty; no update manifest is written, and the app will not update itself")
    if subprocess.run(["xcrun", "notarytool", "history", "--keychain-profile", notary], capture_output=True).returncode != 0:
        raise SystemExit(f"error: notarytool profile '{notary}' is missing or invalid")

    run("cargo", "test", "--workspace", "--locked", cwd=root)
    dist = root / "build/dist"
    if dist.exists():
        shutil.rmtree(dist)
    run(sys.executable, str(root / "scripts/package.py"), "--release", "--out", str(dist), cwd=root)
    app = dist / f"{metadata['name']}.app"
    entitlements = subprocess.run(["codesign", "-d", "--entitlements", "-", "--xml", str(app)], capture_output=True).stdout
    if plistlib.loads(entitlements or b"<plist><dict/></plist>").get("com.apple.developer.icloud-container-environment") != "Production":
        raise SystemExit("error: the release app has no Production iCloud entitlement (Developer ID profile missing?)")

    stem = f"Safay-{version}"
    # The app itself is notarized (through the zip package.py wrote) and stapled; the zip is then made again from the
    # stapled app, so it opens offline too. The update archive carries the app without a DMG around it.
    zipped = dist / f"{stem}-macos-{package.arch()}.zip"
    notarize(zipped, notary)
    run("xcrun", "stapler", "staple", str(app))
    run("xcrun", "stapler", "validate", str(app))
    package.zip_app(app, zipped)

    # The update archive, named as `nsh update-manifest` reads it: <name>-<version>-<os>-<arch>.tar.gz. COPYFILE_DISABLE
    # keeps AppleDouble files (which the signature does not cover) out, as `nsh package` does.
    archive = dist / f"{stem}-macos-{package.arch()}.tar.gz"
    archive.unlink(missing_ok=True)
    run("tar", "-czf", str(archive), "-C", str(app.parent), app.name, env={**os.environ, "COPYFILE_DISABLE": "1"})

    dmg = dist / f"{stem}.dmg"
    dmg.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory() as staging:
        shutil.copytree(app, Path(staging) / app.name, symlinks=True)
        os.symlink("/Applications", Path(staging) / "Applications")
        run("hdiutil", "create", "-quiet", "-volname", metadata["name"], "-srcfolder", staging, "-format", "UDZO", "-ov", str(dmg))
    run("codesign", "--force", "--timestamp", "--sign", identity, str(dmg))
    run("codesign", "--verify", "--strict", str(dmg))
    notarize(dmg, notary)
    run("xcrun", "stapler", "staple", str(dmg))
    run("spctl", "--assess", "--type", "open", "--context", "context:primary-signature", "--verbose", str(dmg))

    if manifest:
        # `nsh update-manifest` reads the id, version and key from an nsh.json with "updater": the project's own nsh.json
        # stays without one, so a copy with this release's source is used.
        with tempfile.TemporaryDirectory() as entry:
            entry_config = dict(metadata, updater={"url": update_url, "publicKey": package.UPDATE_PUBLIC_KEY})
            (Path(entry) / "nsh.json").write_text(json.dumps(entry_config), encoding="utf-8")
            notes = Path(entry) / "notes.md"
            notes.write_text(changelog_section(version) or "", encoding="utf-8")
            command = ["update-manifest", "--entry", entry, "--key", str(secret), "--notes", str(notes), "-o", str(dist), str(archive)]
            if config["download_url_prefix"]:
                command[1:1] = ["--base-url", config["download_url_prefix"]]
            run(sys.executable, str(root / "scripts/nsh.py"), *command, cwd=root)

    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    (dist / f"{stem}.env").write_text(
        f"VERSION={version}\nBUILD={build}\nDMG={dmg.name}\nDMG_SHA256={digest(dmg)}\n"
        f"ZIP={zipped.name}\nZIP_SHA256={digest(zipped)}\n"
        f"UPDATE_ARCHIVE={archive.name}\nUPDATE_ARCHIVE_SHA256={digest(archive)}\nSIGNED=yes\nNOTARIZED=yes\nMANIFEST={'latest.json' if manifest else 'none'}\n",
        encoding="utf-8",
    )
    print((dist / f"{stem}.env").read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
