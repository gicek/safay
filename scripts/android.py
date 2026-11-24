#!/usr/bin/env python3
"""The Android package: `nsh package --target android` from a copy of nsh.json that claims this build's sign-in
callbacks.

A cloud service ends its sign-in by opening a link of the redirect URI's scheme, which Android delivers only to an app
whose manifest claims that scheme. The schemes come from the client IDs in the local `cloud-keys.toml`, which is not
tracked, so the tracked nsh.json cannot list them: this writes `build/android/app/nsh.json`, the project's with those
schemes added and its paths pointing back at the project, and packages from there.

Output: `build/android/debug/Safay.apk` with the development app ID (`--release`: `build/android/release`, the app's own
ID). Other arguments go to `nsh package` (`--run`, `--keystore …`, `--format aab`). The launcher icon is the adaptive
icon in assets/android/.
"""
from pathlib import Path
import argparse
import json
import os
import subprocess
import sys
import tomllib

root = Path(__file__).resolve().parent.parent


def callback_schemes() -> list[str]:
    """The schemes of the redirect URIs a phone's sign-ins use (not the desktop's loopback client)."""
    path = root / "cloud-keys.toml"
    if not path.is_file():
        return []
    keys = tomllib.loads(path.read_text(encoding="utf-8"))
    schemes = []
    for section, client in (("dropbox", "app_key"), ("microsoft", "client_id"), ("google", "client_id")):
        values = keys.get(section, {})
        redirect = str(values.get("redirect_uri", "")).strip()
        if not str(values.get(client, "")).strip() or ":" not in redirect:
            continue
        scheme = redirect.split(":", 1)[0].lower()
        if scheme not in ("http", "https") and scheme not in schemes:
            schemes.append(scheme)
    return schemes


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--release", action="store_true", help="the app's own ID, in build/android/release")
    parser.add_argument(
        "--demo",
        action="store_true",
        help="a development package of its own (ID .demo, \"Safay Demo\"), in build/android/demo: tried on a device without touching the development package's vault",
    )
    args, rest = parser.parse_known_args()
    project = json.loads((root / "nsh.json").read_text(encoding="utf-8"))
    app = root / "build/android/app"
    app.mkdir(parents=True, exist_ok=True)
    back = Path("../../..")
    manifest = dict(project)
    manifest["rust"] = str(back / project["rust"])
    if isinstance(project.get("theme"), str):
        manifest["theme"] = str(back / project["theme"])
    # The launcher icon: the layers of an adaptive icon (scripts/android_icon.py derives them from the app icon), not the
    # app icon itself, which a launcher would shrink inside its own shape.
    icon = back / "assets/android"
    manifest["icon"] = {
        "android": {
            "foreground": str(icon / "foreground.png"),
            "background": "#222222",
            "monochrome": str(icon / "monochrome.png"),
        }
    }
    android = dict(project.get("android", {}))
    android["libraries"] = [str(back / library) for library in android.get("libraries", [])]
    manifest["android"] = android
    manifest["schemes"] = list(dict.fromkeys([*project.get("schemes", []), *callback_schemes()]))
    if args.demo:
        manifest["id"] = project["id"] + ".demo"
        manifest["name"] = project["name"] + " Demo"
    elif not args.release:
        manifest["id"] = project["id"] + ".debug"
    (app / "nsh.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    out = root / "build/android" / ("release" if args.release else "demo" if args.demo else "debug")
    command = [sys.executable, str(root / "scripts/nsh.py"), "package", str(app), "--target", "android", "--out", str(out), *rest]
    # Development packages allow screenshots of the unlocked app (src/mobile/mod.rs), for checking the interface.
    env = dict(os.environ)
    if args.release:
        env.pop("SAFAY_ALLOW_SCREENSHOTS", None)
    else:
        env["SAFAY_ALLOW_SCREENSHOTS"] = "1"
    raise SystemExit(subprocess.call(command, cwd=root, env=env))


if __name__ == "__main__":
    main()
