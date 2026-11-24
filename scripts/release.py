#!/usr/bin/env python3
"""A release with NSH's updater, run on each system for its own package. macOS: tests, a Developer ID build with the
Developer ID profile (iCloud Production), the app notarized and stapled, its update archive, a signed DMG (notarized
and stapled too). Linux and Windows: tests and the release package's archive (`scripts/package.py --release`), which is
also its update archive. With an update source, the signed update manifest (`nsh update-manifest`) lists every
platform's archive of this version in build/dist: copy the other systems' archives there and write it last (or again
with `--manifest-only`) on the machine that holds the update-signing key.

Nothing is uploaded. Settings come from the untracked `local.toml` (environment variables override; see
docs/DEVELOPMENT.md). The manifest is written only when `update_url` and `update_key` are set: the source is the NSH
build's own.

Outputs in build/dist/: on macOS `Safay.app` (notarized and stapled), `Safay-<version>.dmg` (first installs),
`Safay-<version>-macos-<arch>.zip` (the stapled app) and `Safay-<version>-macos-<arch>.tar.gz` (the update archive); on
Linux `Safay-<version>-linux-<arch>.tar.gz`, on Windows `Safay-<version>-windows-<arch>.zip`; `latest.json` and
`latest.json.sig` (with an update source and the key; publish them next to the archives, or under
`download_url_prefix`), and `Safay-<version>-<os>.env` (version, build, hashes, what was done). A release removes only
its own system's earlier outputs and the manifest, so the files copied from the others stay.

Android (`--android`, on macOS): tests, then one APK for each ABI, `Safay-<version>-android-<abi>.apk`, signed with
the app-signing key, for direct installs, and `Safay-<version>-android.aab`, signed with the upload key, for Google Play
(which delivers each device its ABI). Play App Signing holds the same
app-signing key, so either channel's install updates the other's. Each package's certificate, app ID, target API level
and release build are checked against what local.toml [android] names.

The manifest's notes are the version's section of CHANGELOG.md followed by its translations in locales/release-notes/;
a release stops when one of them is missing. `--manifest-only` writes the manifest again for the update archive already
in build/dist (after the version's notes changed), without building, signing or notarizing anything.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import platform
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


# The update notes' translations: locales/release-notes/<code>.md, sectioned as CHANGELOG.md.
NOTE_LANGUAGES = ["zh-Hans", "zh-Hant"]


def changelog_section(version: str, path: Path | None = None) -> str | None:
    """This version's notes from CHANGELOG.md, or from a translation (`## [<version>]` up to the next `## `)."""
    path = path or root / "CHANGELOG.md"
    if not path.is_file():
        return None
    lines, found = [], False
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("## "):
            if found:
                break
            found = line.removeprefix("## ").split(" ", 1)[0].strip("[]") == version
            continue
        if found:
            lines.append(line)
    return "\n".join(lines).strip() if found else None


def update_notes(version: str) -> str:
    """The manifest's notes: the English section, then each translation after a `<!-- lang: <code> -->` line. The app
    shows the part in its own language (`pages::update::notes_in`); the signature covers them all."""
    parts = [changelog_section(version) or ""]
    for code in NOTE_LANGUAGES:
        parts.append(f"<!-- lang: {code} -->\n\n{changelog_section(version, translation(code))}")
    return "\n\n".join(parts) + "\n"


def translation(code: str) -> Path:
    return root / "locales" / "release-notes" / f"{code}.md"


def missing_notes(version: str) -> list[str]:
    """Where this version's notes are missing: CHANGELOG.md or a translation."""
    files = [root / "CHANGELOG.md"] + [translation(code) for code in NOTE_LANGUAGES]
    return [str(f.relative_to(root)) for f in files if changelog_section(version, f) is None]


# The systems' names in archive names and update manifests (`nsh_updater::platform`).
SYSTEMS = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}


def archives(dist: Path, version: str) -> list[Path]:
    """Every platform's update archive of `version` in `dist` (`Safay-<version>-<os>-<arch>.tar.gz` or `.zip`)."""
    names = [f"Safay-{version}-{system}-" for system in SYSTEMS.values()]
    return sorted(p for p in dist.iterdir() if any(p.name.startswith(n) for n in names) and p.name.endswith((".tar.gz", ".zip")))


def clean(dist: Path, system: str) -> None:
    """Removes this system's earlier outputs from `dist`, and, for a desktop system, the manifest, whose hashes would no
    longer match; the other systems' files (copied in for the manifest) stay."""
    if not dist.exists():
        return
    def ours(name: str) -> bool:
        # The manifest lists the desktop archives only: Android's outputs leave it in place.
        if name in ("latest.json", "latest.json.sig"):
            return system != "android"
        if name.startswith("Safay-") and f"-{system}" in name:
            return True
        # macOS's app and DMG, and the `.env` of releases before each system had its own.
        return system == "macos" and (name == "Safay.app" or (name.startswith("Safay-") and name.endswith((".dmg", ".env")) and name.count("-") == 1))
    for path in dist.iterdir():
        if ours(path.name):
            shutil.rmtree(path) if path.is_dir() and not path.is_symlink() else path.unlink()


def write_manifest(metadata: dict, version: str, update_url: str, secret: Path, dist: Path, config: dict) -> None:
    """`latest.json` and `latest.json.sig` for every platform's archive of `version` in `dist`, with this version's update
    notes."""
    found = archives(dist, version)
    if not found:
        raise SystemExit(f"error: no update archive of {version} in {dist.relative_to(root)}")
    print("manifest platforms:", ", ".join(p.name for p in found))
    # `nsh update-manifest` reads the id, version and key from an nsh.json with "updater": the project's own nsh.json
    # stays without one, so a copy with this release's source is used.
    with tempfile.TemporaryDirectory() as entry:
        entry_config = dict(metadata, updater={"url": update_url, "publicKey": package.UPDATE_PUBLIC_KEY})
        (Path(entry) / "nsh.json").write_text(json.dumps(entry_config), encoding="utf-8")
        # The sections' Markdown as it is: the update window renders the one in the app's language.
        notes = Path(entry) / "notes.md"
        notes.write_text(update_notes(version), encoding="utf-8")
        command = ["update-manifest", "--entry", entry, "--key", str(secret), "--notes", str(notes), "-o", str(dist), *map(str, found)]
        if config["download_url_prefix"]:
            command[1:1] = ["--base-url", config["download_url_prefix"]]
        run(sys.executable, str(root / "scripts/nsh.py"), *command, cwd=root)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def release_package(metadata: dict, version: str, build: str, system: str, dist: Path, sign: bool, update_url: str, secret: Path | None, config: dict) -> None:
    """Linux and Windows: tests, the release package (its folder and archive), and the manifest where the key is."""
    run("cargo", "test", "--workspace", "--locked", cwd=root)
    clean(dist, system)
    with tempfile.TemporaryDirectory() as out:
        run(sys.executable, str(root / "scripts/package.py"), "--release", "--out", out, cwd=root)
        built = [p for p in Path(out).iterdir() if p.name.startswith(f"Safay-{version}-{system}-")]
        if len(built) != 1:
            raise SystemExit(f"error: expected one {system} archive from scripts/package.py, found {[p.name for p in built]}")
        dist.mkdir(parents=True, exist_ok=True)
        archive = dist / built[0].name
        shutil.move(built[0], archive)
    if sign:
        write_manifest(metadata, version, update_url, secret, dist, config)
    env = dist / f"Safay-{version}-{system}.env"
    env.write_text(
        f"VERSION={version}\nBUILD={build}\nUPDATE_ARCHIVE={archive.name}\nUPDATE_ARCHIVE_SHA256={digest(archive)}\n"
        f"UPDATER={'yes' if update_url else 'no'}\nMANIFEST={'latest.json' if sign else 'none'}\n",
        encoding="utf-8",
    )
    print(env.read_text(encoding="utf-8"))


# The Android packages' target API level: Google Play's current requirement (nsh.json `android.targetSdk`).
ANDROID_TARGET_SDK = 36
# One APK for each ABI phones use; the App Bundle carries both.
ANDROID_ABIS = ["arm64-v8a", "armeabi-v7a"]


def keychain_password(service: str) -> str:
    """A generic password of the login keychain, by service name; never printed."""
    found = subprocess.run(["security", "find-generic-password", "-a", os.environ.get("USER", ""), "-s", service, "-w"], capture_output=True, text=True)
    if found.returncode != 0 or not found.stdout.strip():
        raise SystemExit(f"error: no keychain password '{service}' (security add-generic-password -a \"$USER\" -s {service} -w)")
    return found.stdout.rstrip("\n")


def certificate_sha256(keystore: Path, alias: str, password: str) -> str:
    """The SHA-256 fingerprint of `alias`'s certificate in `keystore`, as lowercase hex."""
    listed = subprocess.run(
        ["keytool", "-list", "-v", "-keystore", str(keystore), "-alias", alias, "-storepass:env", "SAFAY_KEYSTORE_PASSWORD"],
        capture_output=True, text=True, env={**os.environ, "SAFAY_KEYSTORE_PASSWORD": password},
    )
    for line in listed.stdout.splitlines():
        if line.strip().startswith("SHA256:"):
            return line.split(":", 1)[1].replace(":", "").strip().lower()
    raise SystemExit(f"error: cannot read the certificate of '{alias}' in {keystore} (wrong alias or password?)")


def build_tool(name: str) -> Path:
    """The newest Android build tools' `name` (apksigner, aapt2)."""
    sdk = Path(os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT") or Path.home() / "Library/Android/sdk")
    found = sorted((sdk / "build-tools").glob(f"*/{name}"))
    if not found:
        raise SystemExit(f"error: no {name} in {sdk / 'build-tools'}")
    return found[-1]


def release_android(metadata: dict, version: str, build: str, dist: Path, config: dict) -> None:
    """The signed APK and App Bundle in `dist`, each checked against the key it should carry."""
    keys = {}
    for kind, prefix in (("app", "android_"), ("upload", "android_upload_")):
        keystore, alias, service = config[f"{prefix}keystore"], config[f"{prefix}key_alias"], config[f"{prefix}keychain"]
        if not (keystore and alias and service):
            raise SystemExit(f"error: set local.toml [android] {prefix.removeprefix('android_')}keystore, key_alias and keychain")
        path = Path(os.path.expanduser(keystore))
        if not path.is_file():
            raise SystemExit(f"error: {path} is missing")
        password = keychain_password(service)
        keys[kind] = (path, alias, password, certificate_sha256(path, alias, password))
    if keys["app"][3] == keys["upload"][3]:
        raise SystemExit("error: the upload key must not be the app-signing key")

    run("cargo", "test", "--workspace", "--locked", cwd=root)
    clean(dist, "android")
    dist.mkdir(parents=True, exist_ok=True)
    run("make", "build/android/java/classes.dex", cwd=root)
    out = root / "build/android/release"
    stem = f"Safay-{version}-android"
    packages = [("app", ["--abi", abi], "apk", f"{stem}-{abi}.apk") for abi in ANDROID_ABIS]
    packages.append(("upload", ["--format", "aab"], "aab", f"{stem}.aab"))
    for kind, extra, suffix, name in packages:
        path, alias, password, _ = keys[kind]
        for old in out.glob(f"*.{suffix}"):
            old.unlink()
        run(sys.executable, str(root / "scripts/android.py"), "--release", *extra, "--keystore", str(path), "--key-alias", alias,
            cwd=root, env={**os.environ, "NSH_KEYSTORE_PASSWORD": password})
        built = sorted(out.glob(f"*.{suffix}"))
        if len(built) != 1:
            raise SystemExit(f"error: expected one .{suffix} in {out.relative_to(root)}, found {len(built)}")
        shutil.copy2(built[0], dist / name)
    apks, aab = [dist / f"{stem}-{abi}.apk" for abi in ANDROID_ABIS], dist / f"{stem}.aab"

    # Each APK: the app-signing key's certificate, the app's own ID, the target level, its one ABI, no debuggable flag.
    for abi, apk in zip(ANDROID_ABIS, apks):
        printed = subprocess.run([str(build_tool("apksigner")), "verify", "--print-certs", str(apk)], capture_output=True, text=True, check=True).stdout
        signer = next((line.rsplit(":", 1)[1].strip().lower() for line in printed.splitlines() if "certificate SHA-256 digest" in line), "")
        if signer != keys["app"][3]:
            raise SystemExit(f"error: {apk.name} is not signed with the app-signing key")
        badging = subprocess.run([str(build_tool("aapt2")), "dump", "badging", str(apk)], capture_output=True, text=True, check=True).stdout
        expected = [f"package: name='{metadata['id']}'", f"versionCode='{build}'", f"versionName='{version}'",
                    f"targetSdkVersion:'{ANDROID_TARGET_SDK}'", f"native-code: '{abi}'"]
        if missing := [e for e in expected if e not in badging]:
            raise SystemExit(f"error: {apk.name} lacks {missing}")
        if "application-debuggable" in badging:
            raise SystemExit(f"error: {apk.name} is debuggable")
    # The bundle: the upload key's certificate.
    printed = subprocess.run(["keytool", "-printcert", "-jarfile", str(aab)], capture_output=True, text=True).stdout
    fingerprint = next((line.split(":", 1)[1].replace(":", "").strip().lower() for line in printed.splitlines() if line.strip().startswith("SHA256:")), "")
    if fingerprint != keys["upload"][3]:
        raise SystemExit("error: the App Bundle is not signed with the upload key")

    listed = "".join(f"APK_{abi.upper().replace('-', '_')}={apk.name}\nAPK_{abi.upper().replace('-', '_')}_SHA256={digest(apk)}\n" for abi, apk in zip(ANDROID_ABIS, apks))
    (dist / f"{stem}.env").write_text(
        f"VERSION={version}\nBUILD={build}\n{listed}AAB={aab.name}\nAAB_SHA256={digest(aab)}\n"
        f"APP_SIGNING_CERT_SHA256={keys['app'][3]}\nUPLOAD_CERT_SHA256={keys['upload'][3]}\n",
        encoding="utf-8",
    )
    print((dist / f"{stem}.env").read_text(encoding="utf-8"))


def notarize(path: Path, profile: str) -> None:
    run("xcrun", "notarytool", "submit", str(path), "--keychain-profile", profile, "--wait")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--manifest-only", action="store_true", help="write the manifest again for build/dist's update archive")
    parser.add_argument("--notes", action="store_true", help="print this version's update notes, as the manifest carries them")
    parser.add_argument("--android", action="store_true", help="the signed Android APK and App Bundle (on macOS)")
    args = parser.parse_args()
    metadata = json.loads((root / "nsh.json").read_text(encoding="utf-8"))
    version, build = metadata["version"], str(metadata["build"])
    if args.notes:
        if missing := missing_notes(version):
            raise SystemExit(f"error: no '## [{version}]' section for the update notes in {', '.join(missing)}")
        print(update_notes(version), end="")
        return
    config = package.settings()
    if args.android:
        if metadata.get("android", {}).get("targetSdk") != ANDROID_TARGET_SDK:
            raise SystemExit(f"error: nsh.json android.targetSdk must be {ANDROID_TARGET_SDK}")
        release_android(metadata, version, build, root / "build/dist", config)
        return
    system = SYSTEMS.get(platform.system())
    if system is None:
        raise SystemExit(f"error: no release for {platform.system()}")
    identity, notary = config["identity"], config["notary_profile"]
    update_url, update_key = config["update_url"], config["update_key"]
    if system == "macos" and not args.manifest_only:
        if not identity.startswith("Developer ID Application"):
            raise SystemExit("error: set local.toml [signing] identity to a Developer ID Application certificate")
        if not notary:
            raise SystemExit("error: set local.toml [release] notary_profile (xcrun notarytool store-credentials)")
    manifest = bool(update_url)
    secret = None
    if manifest:
        if not update_url.startswith("https://"):
            raise SystemExit("error: update_url must be https")
        if not package.UPDATE_PUBLIC_KEY:
            raise SystemExit(
                "error: no update-signing key yet: run `python3 scripts/nsh.py keygen <path>/safay-update`, set local.toml "
                "[release] update_key to <path>/safay-update.secret and scripts/package.py UPDATE_PUBLIC_KEY to the .pub content"
            )
        if missing := missing_notes(version):
            raise SystemExit(f"error: no '## [{version}]' section for the update notes in {', '.join(missing)}")
        secret = Path(os.path.expanduser(update_key)) if update_key else None
        if secret is not None and secret.is_file():
            # A manifest signed with another key would make every installed copy refuse the update: check the pair first.
            public = secret.with_suffix(".pub")
            if not public.is_file() or public.read_text(encoding="utf-8").strip() != package.UPDATE_PUBLIC_KEY:
                raise SystemExit(f"error: {public} does not hold the public key in scripts/package.py")
        elif args.manifest_only or system == "macos":
            raise SystemExit("error: set local.toml [release] update_key to the update-signing secret (nsh keygen)")
        else:
            # The package carries the source and the public key; the manifest is signed where the secret is.
            print("note: no update-signing key here; copy the archive to build/dist on the machine with the key, then run --manifest-only")
            secret = None
    else:
        print("note: update_url is empty; no update manifest is written, and the app will not update itself")
    dist = root / "build/dist"
    if args.manifest_only:
        if not manifest:
            raise SystemExit("error: --manifest-only needs an update source (update_url)")
        write_manifest(metadata, version, update_url, secret, dist, config)
        return
    if system != "macos":
        release_package(metadata, version, build, system, dist, manifest and secret is not None, update_url, secret, config)
        return
    if subprocess.run(["xcrun", "notarytool", "history", "--keychain-profile", notary], capture_output=True).returncode != 0:
        raise SystemExit(f"error: notarytool profile '{notary}' is missing or invalid")

    run("cargo", "test", "--workspace", "--locked", cwd=root)
    clean(dist, "macos")
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
        write_manifest(metadata, version, update_url, secret, dist, config)

    (dist / f"{stem}-macos.env").write_text(
        f"VERSION={version}\nBUILD={build}\nDMG={dmg.name}\nDMG_SHA256={digest(dmg)}\n"
        f"ZIP={zipped.name}\nZIP_SHA256={digest(zipped)}\n"
        f"UPDATE_ARCHIVE={archive.name}\nUPDATE_ARCHIVE_SHA256={digest(archive)}\nSIGNED=yes\nNOTARIZED=yes\nMANIFEST={'latest.json' if manifest else 'none'}\n",
        encoding="utf-8",
    )
    print((dist / f"{stem}-macos.env").read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
