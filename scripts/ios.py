#!/usr/bin/env python3
"""The iOS device package: `nsh package --target ios` signed with the development identity and the iOS provisioning
profile from the local `local.toml` (`[ios] profile`), from a copy of nsh.json that adds what the profile allows.

iCloud backup needs the iCloud entitlement, which only a provisioning profile grants, and the app creates its CloudKit
container only when Info.plist names it (`src/platform/icloud_apple.rs`). A profile carrying the container
`iCloud.com.gicek.safay` gets both: this writes `build/ios/app/nsh.json`, the project's with `SafayICloudContainer` in
`ios.infoPlist` and the iCloud entitlements in `ios.entitlements` (CloudKit only, in the environment of the profile's
kind), its paths pointing back at the project, and packages from there. The tracked nsh.json stays without them: the
simulator's builds are not signed with a profile, and a CloudKit container without the entitlement ends the app.

Output: `build/ios/debug/Safay.ipa` with the development app ID. `--run` installs it on the connected device (or the one
`--device` names, its UDID) and launches it. Other arguments go to `nsh package`.

`--simulator` packages for the simulator instead (`build/ios-simulator/debug/Safay.app`, signed ad hoc; `--run` installs
and launches it in the booted simulator). An app with no entitlements cannot write the Keychain there
(`errSecMissingEntitlement`), where cloud sign-ins are kept, and the simulator refuses to launch an ad-hoc app whose
signature carries any. So the entitlements go where Xcode puts a simulator app's: sections of the executable
(`__TEXT,__entitlements` as XML, `__ents_der` as DER from `derq`), linked in through the simulator target's RUSTFLAGS.
The simulator checks the application identifier against no team.
"""
from pathlib import Path
import argparse
import json
import os
import plistlib
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import package  # noqa: E402  (local.toml settings)

root = Path(__file__).resolve().parent.parent
ICLOUD_CONTAINER = package.ICLOUD_CONTAINER
DEVELOPER = {"DEVELOPER_DIR": os.environ.get("DEVELOPER_DIR", "/Applications/Xcode.app/Contents/Developer")}


def profile(path: Path) -> dict:
    decoded = subprocess.run(["security", "cms", "-D", "-i", str(path)], capture_output=True, check=True)
    return plistlib.loads(decoded.stdout)


def connected_device() -> str:
    """The UDID of the one connected iPhone or iPad (not a simulator)."""
    listing = subprocess.run(
        ["xcrun", "devicectl", "list", "devices", "--json-output", "/dev/stdout", "--quiet"],
        capture_output=True,
        text=True,
        env={**os.environ, **DEVELOPER},
    ).stdout
    start = listing.find("{")
    devices = json.loads(listing[start:])["result"]["devices"] if start >= 0 else []
    real = [
        d["hardwareProperties"]["udid"]
        for d in devices
        if d.get("hardwareProperties", {}).get("reality") == "physical" and d.get("connectionProperties", {}).get("pairingState") == "paired"
    ]
    if len(real) != 1:
        raise SystemExit(f"pass --device <UDID>: {len(real)} paired devices connected")
    return real[0]


def copy_manifest(project: dict, bundle_id: str) -> tuple[Path, dict]:
    """The directory packaged from (`build/ios/app`) and the project's nsh.json for it: paths pointing back at the
    project, the development app ID."""
    app = root / "build/ios/app"
    app.mkdir(parents=True, exist_ok=True)
    back = Path("../../..")
    manifest = dict(project)
    manifest["rust"] = str(back / project["rust"])
    if isinstance(project.get("theme"), str):
        manifest["theme"] = str(back / project["theme"])
    manifest["id"] = bundle_id
    return app, manifest


def simulator(run: bool, rest: list[str]) -> None:
    project = json.loads((root / "nsh.json").read_text(encoding="utf-8"))
    bundle_id = project["id"] + ".debug"
    app, manifest = copy_manifest(project, bundle_id)
    (app / "nsh.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    out = root / "build/ios-simulator"
    out.mkdir(parents=True, exist_ok=True)
    identifier = f"SIMULATOR.{bundle_id}"
    xml, der = out / "entitlements.plist", out / "entitlements.der"
    xml.write_bytes(plistlib.dumps({"application-identifier": identifier, "keychain-access-groups": [identifier]}))
    subprocess.run(["derq", "query", "-f", "xml", "-i", str(xml), "-o", str(der), "--raw"], check=True)
    sections = " ".join(f"-Clink-arg=-Wl,-sectcreate,__TEXT,{name},{path}" for name, path in (("__entitlements", xml), ("__ents_der", der)))
    env = {**os.environ, **DEVELOPER, "CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUSTFLAGS": sections}
    command = [
        sys.executable,
        str(root / "scripts/nsh.py"),
        "package",
        str(app),
        "--target",
        "ios-simulator",
        "--icon",
        str(root / "assets/ios/icon.png"),
        "--out",
        str(root / "build/ios-simulator/debug"),
        *(["--run"] if run else []),
        *rest,
    ]
    raise SystemExit(subprocess.call(command, cwd=root, env=env))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--run", action="store_true", help="install it on the connected device and launch it")
    parser.add_argument("--device", help="the device's UDID (default: the one paired device connected)")
    parser.add_argument("--simulator", action="store_true", help="package for the simulator instead")
    args, rest = parser.parse_known_args()
    if args.simulator:
        return simulator(args.run, rest)
    settings = package.settings()
    identity = settings["development_identity"]
    profile_path = Path(os.path.expanduser(settings.get("ios_profile", ""))) if settings.get("ios_profile") else None
    if not identity or not profile_path or not profile_path.is_file():
        raise SystemExit("an iOS device package needs local.toml [signing] development_identity and [ios] profile (a .mobileprovision)")
    project = json.loads((root / "nsh.json").read_text(encoding="utf-8"))
    bundle_id = project["id"] + ".debug"
    granted = profile(profile_path)
    entitlements = granted.get("Entitlements", {})
    if entitlements.get("application-identifier", "").split(".", 1)[-1] not in (bundle_id, "*"):
        raise SystemExit(f"{profile_path.name} is not for {bundle_id}")

    app, manifest = copy_manifest(project, bundle_id)
    ios = dict(project.get("ios", {}))
    if ICLOUD_CONTAINER in entitlements.get("com.apple.developer.icloud-container-identifiers", []):
        # A development profile's apps use CloudKit's Development environment; the profile lists both and grants
        # every service (`*`), which the app narrows to what it uses.
        # A backup changed on another device arrives as a silent push, which wakes the app in the background only with
        # this mode.
        modes = list(dict.fromkeys([*ios.get("infoPlist", {}).get("UIBackgroundModes", []), "remote-notification"]))
        ios["infoPlist"] = {**ios.get("infoPlist", {}), "SafayICloudContainer": ICLOUD_CONTAINER, "UIBackgroundModes": modes}
        ios["entitlements"] = {
            **ios.get("entitlements", {}),
            "com.apple.developer.icloud-container-identifiers": [ICLOUD_CONTAINER],
            "com.apple.developer.icloud-services": ["CloudKit"],
            "com.apple.developer.icloud-container-environment": "Development",
        }
    else:
        print(f"note: {profile_path.name} has no {ICLOUD_CONTAINER}: packaging without iCloud")
    manifest["ios"] = ios
    (app / "nsh.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    out = root / "build/ios/debug"
    command = [
        sys.executable,
        str(root / "scripts/nsh.py"),
        "package",
        str(app),
        "--target",
        "ios",
        "--icon",
        str(root / "assets/ios/icon.png"),
        "--sign",
        identity,
        "--profile",
        str(profile_path),
        "--out",
        str(out),
        *rest,
    ]
    env = {**os.environ, **DEVELOPER, "SAFAY_ALLOW_SCREENSHOTS": "1"}
    code = subprocess.call(command, cwd=root, env=env)
    if code or not args.run:
        raise SystemExit(code)
    device = args.device or connected_device()
    ipa = next(out.glob("*.ipa"))
    # `--device` before the positional arguments: after the bundle ID it would go to the app as an argument.
    xcrun = lambda *verb, target: subprocess.call(
        ["xcrun", "devicectl", "device", *verb, "--device", device, target], env={**os.environ, **DEVELOPER}
    )
    raise SystemExit(xcrun("install", "app", target=str(ipa)) or xcrun("process", "launch", target=bundle_id))


if __name__ == "__main__":
    main()
