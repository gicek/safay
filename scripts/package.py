#!/usr/bin/env python3
"""Package our native host.

macOS signing: development builds sign with the "Apple Development" identity and the
development provisioning profile (iCloud in CloudKit's Development environment); `--release` signs with Developer ID,
Hardened Runtime, a secure timestamp and the Developer ID profile (iCloud Production). Without an identity or profile
the bundle is ad-hoc signed for this Mac only and has no iCloud. Settings come from the untracked `local.toml`, and
environment variables override them (docs/DEVELOPMENT.md). A release package with an update source (`update_url`), or
a development package given `SAFAY_UPDATE_URL`, carries NSH's updater files (`nsh/app.json`,
`nsh/assets/nsh-updater.json`), which `nsh-updater` requires to replace the app.

Linux and Windows packages are a folder (`Safay/`, the executable at its top) and its archive, named as the update
manifest reads it: `Safay-<version>-linux-<arch>.tar.gz`, `Safay-<version>-windows-<arch>.zip`. The updater replaces
that folder wherever the user unpacked it.

Output: `build/<system>/debug/` (development) or `build/<system>/release/` (`--release`, which on macOS also writes
`Safay-<version>-macos-<arch>.zip`), `<system>` being `macos`, `linux` or `windows`; `--out` picks another folder.
"""
from pathlib import Path
import argparse
import datetime
import json
import os
import platform
import plistlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile

root = Path(__file__).resolve().parents[1]

# The app's CloudKit container.
ICLOUD_CONTAINER = "iCloud.com.gicek.safay"
# The update-signing public key, the `.pub` that `nsh keygen` writes next to the secret local.toml's `update_key` names
# (docs/DEVELOPMENT.md, "Releases and updates").
UPDATE_PUBLIC_KEY = "zLYqjrlhIwCj9vVIWcFX4PkGSGJnlCSOeI0BEPe59Xo="
PROFILE_DIRS = [Path.home() / "Library/Developer/Xcode/UserData/Provisioning Profiles", Path.home() / "Library/MobileDevice/Provisioning Profiles"]


def settings() -> dict:
    """`local.toml` merged with environment overrides. Missing file: everything empty (ad-hoc)."""
    try:
        data = tomllib.loads((root / "local.toml").read_text(encoding="utf-8"))
    except FileNotFoundError:
        data = {}
    signing, release, android = data.get("signing", {}), data.get("release", {}), data.get("android", {})
    pick = lambda env, value: os.environ.get(env, value or "")
    return {
        "identity": pick("SAFAY_SIGN_IDENTITY", signing.get("identity")),
        "development_identity": pick("SAFAY_DEVELOPMENT_IDENTITY", signing.get("development_identity")),
        "team": pick("SAFAY_TEAM_ID", signing.get("team")),
        "notary_profile": pick("SAFAY_NOTARY_PROFILE", release.get("notary_profile")),
        "update_url": pick("SAFAY_UPDATE_URL", release.get("update_url")),
        "update_key": pick("SAFAY_UPDATE_KEY", release.get("update_key")),
        "download_url_prefix": pick("SAFAY_DOWNLOAD_URL_PREFIX", release.get("download_url_prefix")),
        # Android: the app-signing key (the APK; Play App Signing holds the same key) and the upload key (the bundle for
        # Google Play). Each password comes from the login keychain's generic password of that service name.
        "android_keystore": pick("SAFAY_ANDROID_KEYSTORE", android.get("keystore")),
        "android_key_alias": pick("SAFAY_ANDROID_KEY_ALIAS", android.get("key_alias")),
        "android_keychain": pick("SAFAY_ANDROID_KEYCHAIN", android.get("keychain")),
        "android_upload_keystore": pick("SAFAY_ANDROID_UPLOAD_KEYSTORE", android.get("upload_keystore")),
        "android_upload_key_alias": pick("SAFAY_ANDROID_UPLOAD_KEY_ALIAS", android.get("upload_key_alias")),
        "android_upload_keychain": pick("SAFAY_ANDROID_UPLOAD_KEYCHAIN", android.get("upload_keychain")),
        # iOS devices: the development provisioning profile (`.mobileprovision`) of the development app ID, with the
        # devices it installs on (scripts/ios.py).
        "ios_profile": pick("SAFAY_IOS_PROFILE", data.get("ios", {}).get("profile")),
    }


def find_profile(team: str, bundle_id: str, developer_id: bool) -> Path | None:
    """The newest valid profile for this App ID: Developer ID ("Direct", no device list) or development (device list)."""
    wanted = f"{team}.{bundle_id}"
    now = datetime.datetime.now(datetime.timezone.utc)
    best = None
    for folder in PROFILE_DIRS:
        for path in folder.glob("*.provisionprofile"):
            decoded = subprocess.run(["security", "cms", "-D", "-i", str(path)], capture_output=True)
            if decoded.returncode != 0:
                continue
            profile = plistlib.loads(decoded.stdout)
            entitlements = profile.get("Entitlements", {})
            if entitlements.get("com.apple.application-identifier") != wanted:
                continue
            if ("ProvisionedDevices" in profile) == developer_id or profile["ExpirationDate"].replace(tzinfo=datetime.timezone.utc) <= now:
                continue
            if ICLOUD_CONTAINER not in entitlements.get("com.apple.developer.icloud-container-identifiers", []):
                continue
            if best is None or profile["CreationDate"] > best[0]:
                best = (profile["CreationDate"], path)
    return best[1] if best else None


def profile_entitlements(path: Path) -> dict:
    decoded = subprocess.run(["security", "cms", "-D", "-i", str(path)], capture_output=True, check=True)
    return plistlib.loads(decoded.stdout).get("Entitlements", {})


def codesign(identity: str, path: Path, *, release: bool, entitlements: Path | None = None) -> None:
    command = ["codesign", "--force", "--sign", identity]
    if identity != "-":
        command += ["--options", "runtime", "--timestamp" if release else "--timestamp=none"]
    if entitlements:
        command += ["--entitlements", str(entitlements)]
    subprocess.run(command + [str(path)], check=True)


def update_source(release: bool) -> str:
    """The update manifest's URL a package carries: release packages update from the configured source. Development
    packages are a separate app (`<id>.debug`) that the release manifest does not name: they carry an updater only for a
    source set explicitly in SAFAY_UPDATE_URL (local end-to-end tests). Empty: no updater."""
    update_url = settings()["update_url"] if release else os.environ.get("SAFAY_UPDATE_URL", "")
    if update_url and not (os.environ.get("SAFAY_UPDATE_PUBLIC_KEY") or UPDATE_PUBLIC_KEY):
        print("note: scripts/package.py has no UPDATE_PUBLIC_KEY yet; the updater files are left out")
        return ""
    return update_url


def write_updater(resources: Path, bundle_id: str, version: str, update_url: str) -> None:
    """NSH's updater files (`nsh-updater`) in the package's `nsh` folder: the source and key come from the package, never
    from code; `app.json` and the config file are what `Installation::current` checks before it replaces the app."""
    (resources / "assets").mkdir(parents=True, exist_ok=True)
    (resources / "app.json").write_text(json.dumps({"id": bundle_id}), encoding="utf-8")
    # SAFAY_UPDATE_PUBLIC_KEY is for local end-to-end tests with a throwaway key only.
    public_key = os.environ.get("SAFAY_UPDATE_PUBLIC_KEY", UPDATE_PUBLIC_KEY)
    config = {"id": bundle_id, "version": version, "url": update_url, "publicKey": public_key}
    (resources / "assets/nsh-updater.json").write_text(json.dumps(config), encoding="utf-8")


def bundle_id_of(metadata: dict, release: bool) -> str:
    """nsh.json's `id` for releases; development (debug) builds are the separate app `<id>.debug`, as `src/platform`'s
    `APP_ID`, with their own data directory, keychain items and provisioning profile."""
    return metadata["id"] if release else f"{metadata['id']}.debug"


def package_macos(metadata: dict, exe: Path, out: Path, release: bool) -> Path:
    name = metadata["name"]
    bundle_id = bundle_id_of(metadata, release)
    config = settings()
    identity = config["identity"] if release else config["development_identity"]
    identity = identity or "-"
    if release and not identity.startswith("Developer ID Application"):
        raise SystemExit("--release needs a Developer ID Application identity (local.toml [signing] identity)")
    profile = find_profile(config["team"], bundle_id, release) if identity != "-" and config["team"] else None
    if identity != "-" and profile is None:
        print(f"note: no {'Developer ID' if release else 'development'} profile for {bundle_id} with iCloud; signing without iCloud")
    update_url = update_source(release)
    if update_url and identity == "-":
        print("note: an ad-hoc build is not offered updates; the updater files are left out")
        update_url = ""

    app = out / f"{name}.app"
    if app.exists():
        shutil.rmtree(app)
    binary = app / "Contents/MacOS" / name
    binary.parent.mkdir(parents=True)
    resources = app / "Contents/Resources"
    resources.mkdir()
    shutil.copytree(root / "assets", resources / "assets/@safay")
    shutil.copy2(exe, binary)
    # Intermediate files (iconset, entitlements) stay out of the output folder, which holds only what is shipped.
    scratch = Path(tempfile.mkdtemp(prefix="safay-package-"))
    iconset = scratch / "Safay.iconset"
    iconset.mkdir()
    for size in [16, 32, 128, 256, 512]:
        for scale, suffix in [(1, ""), (2, "@2x")]:
            subprocess.run(["sips", "-z", str(size * scale), str(size * scale), str(root / metadata["icon"]), "--out", str(iconset / f"icon_{size}x{size}{suffix}.png")], check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(resources / "Safay.icns")], check=True)
    shutil.copy2(root / "assets/fonts/AtkinsonHyperlegibleMono-OFL.txt", resources / "AtkinsonHyperlegibleMono-OFL.txt")
    for file in ["LICENSE", "THIRD_PARTY_NOTICES.md"]:
        shutil.copy2(root / file, resources / file)
    # The launchers' AppleScript commands (src/platform/automation_macos.rs) and the Alfred workflow, signed with the app.
    shutil.copy2(root / "resources/Safay.sdef", resources / "Safay.sdef")
    subprocess.run([sys.executable, str(root / "scripts/integrations.py"), "--check", "--alfred", str(resources / "Safay.alfredworkflow")], check=True)

    info = {"CFBundleExecutable": name, "CFBundleName": name, "CFBundleDisplayName": name, "CFBundleIdentifier": bundle_id, "CFBundleShortVersionString": metadata["version"], "CFBundleVersion": str(metadata["build"]), "CFBundlePackageType": "APPL", "LSMinimumSystemVersion": "14.0", "NSHighResolutionCapable": True, "CFBundleIconFile": "Safay.icns", "NSPrincipalClass": "NSApplication", "NSCameraUsageDescription": "Safay uses the camera to scan 2FA QR codes. Images are processed on this device only.", "CFBundleURLTypes": [{"CFBundleURLName": bundle_id, "CFBundleURLSchemes": [metadata["schemes"][0]]}], "NSAppleScriptEnabled": True, "OSAScriptingDefinition": "Safay.sdef"}
    entitlements: dict = {}
    if identity != "-":
        # Hardened Runtime: camera access must be declared as an entitlement.
        entitlements["com.apple.security.device.camera"] = True
    if profile:
        # iCloud is a restricted entitlement: it must match the embedded profile. Development builds use CloudKit's
        # Development environment, releases Production, so test builds never touch real backups.
        shutil.copy2(profile, app / "Contents/embedded.provisionprofile")
        entitlements.update({
            "com.apple.application-identifier": f"{config['team']}.{bundle_id}",
            "com.apple.developer.team-identifier": config["team"],
            "com.apple.developer.icloud-container-identifiers": [ICLOUD_CONTAINER],
            "com.apple.developer.icloud-services": ["CloudKit"],
            "com.apple.developer.icloud-container-environment": "Production" if release else "Development",
        })
        # Silent pushes for iCloud backup changes need the push capability on the App ID; a profile without it leaves the
        # entitlement out (pushes fail to register quietly and backups sync on unlock, after saves and by hand).
        push = profile_entitlements(profile).get("com.apple.developer.aps-environment")
        if push:
            entitlements["com.apple.developer.aps-environment"] = push
        else:
            print(f"note: the profile for {bundle_id} has no push capability; iCloud changes arrive without silent push")
        # The app creates its CloudKit container only when this key is present (src/platform/icloud_apple.rs).
        info["SafayICloudContainer"] = ICLOUD_CONTAINER
    if update_url:
        write_updater(app / "Contents/Resources/nsh", bundle_id, metadata["version"], update_url)
    (app / "Contents/Info.plist").write_bytes(plistlib.dumps(info))

    entitlements_file = None
    if entitlements:
        entitlements_file = scratch / "safay.entitlements"
        entitlements_file.write_bytes(plistlib.dumps(entitlements))
    try:
        codesign(identity, app, release=release, entitlements=entitlements_file)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    return app


def arch() -> str:
    """The host architecture as release file names spell it (`aarch64`, `x86_64`)."""
    machine = platform.machine().lower()
    return {"arm64": "aarch64", "amd64": "x86_64"}.get(machine, machine)


def zip_app(app: Path, archive: Path) -> Path:
    """A zip of the app (`ditto`), signatures intact, holding the `.app` and nothing else.

    `--norsrc` keeps extended attributes and resource forks out: `--sequesterRsrc` would add them as AppleDouble files
    in a `__MACOSX` folder beside the app, which the signature does not cover, and NSH's updater refuses an archive whose
    top holds more than the app ("the archive must hold one directory, the app").
    """
    archive.unlink(missing_ok=True)
    subprocess.run(["ditto", "-c", "-k", "--norsrc", "--keepParent", str(app), str(archive)], check=True)
    with zipfile.ZipFile(archive) as zipped:
        tops = {name.split("/", 1)[0] for name in zipped.namelist()}
    if tops != {app.name}:
        raise SystemExit(f"error: {archive.name} must hold only {app.name}, found {sorted(tops)}")
    return archive


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--release", action="store_true", help="Developer ID build with Hardened Runtime (not notarized)")
    parser.add_argument("--out", type=Path, help="output folder (default: build/<system>/debug or build/<system>/release)")
    args = parser.parse_args()
    metadata = json.loads((root / "nsh.json").read_text(encoding="utf-8"))
    name = metadata["name"]
    profile = "release" if args.release else "debug"
    subprocess.run(["cargo", "build", "--locked", *(["--release"] if args.release else [])], cwd=root, check=True)
    system = platform.system()
    folder = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}.get(system, system.lower())
    out = (args.out or root / "build" / folder / profile).resolve()
    out.mkdir(parents=True, exist_ok=True)
    exe = root / "target" / profile / ("safay.exe" if os.name == "nt" else "safay")
    if system == "Darwin":
        app = package_macos(metadata, exe, out, args.release)
        print(app)
        if args.release:
            print(zip_app(app, out / f"{name}-{metadata['version']}-macos-{arch()}.zip"))
        return
    folder = out / name
    # Made afresh: a file an earlier build left would ship, and the updater would install it.
    if folder.exists():
        shutil.rmtree(folder)
    folder.mkdir()
    shutil.copytree(root / "assets", folder / "assets/@safay", dirs_exist_ok=True)
    shutil.copy2(exe, folder / exe.name)
    for file in ["LICENSE", "THIRD_PARTY_NOTICES.md"]:
        shutil.copy2(root / file, folder / file)
    app_id = bundle_id_of(metadata, args.release)
    shutil.copy2(root / metadata["icon"], folder / f"{app_id}.png")
    shutil.copy2(root / "assets/fonts/AtkinsonHyperlegibleMono-OFL.txt", folder / "AtkinsonHyperlegibleMono-OFL.txt")
    if update_url := update_source(args.release):
        write_updater(folder / "nsh", app_id, metadata["version"], update_url)
    if system == "Linux":
        (folder / f"{app_id}.desktop").write_text(f"[Desktop Entry]\nType=Application\nName={name}\nExec=safay\nIcon={app_id}\nCategories=Utility;Security;\nTerminal=false\nMimeType=x-scheme-handler/{metadata['schemes'][0]};\n", encoding="utf-8")
        archive = out / f"{name}-{metadata['version']}-linux-{arch()}.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            output.add(folder, arcname=name)
    elif system == "Windows":
        archive = out / f"{name}-{metadata['version']}-windows-{arch()}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for file in folder.rglob("*"):
                if file.is_file():
                    output.write(file, file.relative_to(out))
    else:
        parser.error(f"Unsupported desktop host: {system}")
    print(archive)


if __name__ == "__main__":
    main()
