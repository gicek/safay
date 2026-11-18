#!/usr/bin/env python3
"""Run the NSH CLI built from the commit Cargo.lock pins; never silently use a CLI from another NSH revision."""
from pathlib import Path
import os
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import install_nsh  # noqa: E402

root = install_nsh.root
commit = install_nsh.pinned_commit()
# The CLI builds the crate in its own workspace (.nsh/rust), whose lock file it creates once and never updates: a new one
# would take NSH main's latest commit, and one from before an upgrade keeps the old commit. It starts from the project's.
generated = root / ".nsh/rust/Cargo.lock"
if install_nsh.locked_commits(generated) != {commit}:
    generated.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / "Cargo.lock", generated)
explicit = os.environ.get("NSH_BIN")
candidate = Path(explicit) if explicit else install_nsh.executable()
if candidate.is_file():
    version = subprocess.check_output([str(candidate), "--version"], text=True).strip()
    if f"({commit[:7]})" in version:
        raise SystemExit(subprocess.call([str(candidate), *sys.argv[1:]], cwd=root))
    raise SystemExit(f"{candidate} is '{version}', but Cargo.lock pins NSH {commit[:7]}. Run: python3 scripts/install_nsh.py")
raise SystemExit(f"No NSH CLI for {commit[:7]}. Run: python3 scripts/install_nsh.py")
