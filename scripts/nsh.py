#!/usr/bin/env python3
"""Run the NSH CLI built from the commit Cargo.lock pins; never silently use a CLI from another NSH revision."""
from pathlib import Path
import os
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import install_nsh  # noqa: E402

root = install_nsh.root
commit = install_nsh.pinned_commit()
explicit = os.environ.get("NSH_BIN")
candidate = Path(explicit) if explicit else install_nsh.executable()
if candidate.is_file():
    version = subprocess.check_output([str(candidate), "--version"], text=True).strip()
    if f"({commit[:7]})" in version:
        raise SystemExit(subprocess.call([str(candidate), *sys.argv[1:]], cwd=root))
    raise SystemExit(f"{candidate} is '{version}', but Cargo.lock pins NSH {commit[:7]}. Run: python3 scripts/install_nsh.py")
raise SystemExit(f"No NSH CLI for {commit[:7]}. Run: python3 scripts/install_nsh.py")
