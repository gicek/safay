#!/usr/bin/env python3
"""Run the NSH CLI built from the commit Cargo.lock pins; never silently use a CLI from another NSH revision."""
from pathlib import Path
import hashlib
import os
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import install_nsh  # noqa: E402

root = install_nsh.root
commit = install_nsh.pinned_commit()
# The CLI builds the crate in its own workspace (.nsh/rust), whose lock file it creates once and never updates: a new one
# would take NSH main's latest commit, and one from before an upgrade keeps the old commit. It starts from the project's,
# and again whenever the project's changes (a new version of Safay included): cargo would resolve a stale lock file
# anew, and move the NSH crates that are not taken from the checkout to main's latest commit while doing so. An app
# directory named on the command line (scripts/android.py's build/android/app) has a workspace of its own, kept the same way.
project = (root / "Cargo.lock").read_bytes()
digest = hashlib.sha256(project).hexdigest()
apps = [root, *(Path(arg).resolve() for arg in sys.argv[1:] if (Path(arg) / "nsh.json").is_file())]
for app in dict.fromkeys(apps):
    generated = app / ".nsh/rust/Cargo.lock"
    origin = generated.with_name("Cargo.lock.origin")
    if install_nsh.locked_commits(generated) != {commit} or not origin.is_file() or origin.read_text(encoding="utf-8") != digest:
        generated.parent.mkdir(parents=True, exist_ok=True)
        generated.write_bytes(project)
        origin.write_text(digest, encoding="utf-8")
explicit = os.environ.get("NSH_BIN")
candidate = Path(explicit) if explicit else install_nsh.executable()
if candidate.is_file():
    version = subprocess.check_output([str(candidate), "--version"], text=True).strip()
    if f"({commit[:7]})" in version:
        raise SystemExit(subprocess.call([str(candidate), *sys.argv[1:]], cwd=root))
    raise SystemExit(f"{candidate} is '{version}', but Cargo.lock pins NSH {commit[:7]}. Run: python3 scripts/install_nsh.py")
raise SystemExit(f"No NSH CLI for {commit[:7]}. Run: python3 scripts/install_nsh.py")
