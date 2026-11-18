#!/usr/bin/env python3
"""Build the NSH CLI from the exact NSH commit that Cargo.lock pins.

The project tracks NSH's `main` branch (AGENTS.md): Cargo.toml takes the `nsh-*` crates from Git, Cargo.lock pins the
commit, and the CLI must be built from that same commit, since the code it generates calls those crates' API. This
follows NSH's official source install (`install.sh --source`): a checkout, `cargo build --release --locked -p nsh-cli`,
and the checkout kept so the CLI can build development hosts and runtimes from the same revision. The checkout lives in
`.nsh/toolchain/source` (generated, untracked). A CLI already built from the pinned commit is kept as it is.

Upgrading NSH: `cargo update -p nsh-core` (moves every nsh crate to the newest main), then run this script again.
"""
from pathlib import Path
import argparse
import re
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
REPOSITORY = "https://github.com/longbridge/nsh"
SOURCE = root / ".nsh/toolchain/source"


def locked_commits(lock: Path) -> set[str]:
    """The NSH commits a lock file resolved the nsh crates to."""
    text = lock.read_text(encoding="utf-8") if lock.is_file() else ""
    return set(re.findall(r'source = "git\+' + re.escape(REPOSITORY) + r'\?branch=main#([0-9a-f]{40})"', text))


def pinned_commit() -> str:
    """The commit Cargo.lock resolved for the nsh crates (all from one checkout)."""
    commits = locked_commits(root / "Cargo.lock")
    if len(commits) != 1:
        raise SystemExit(f"Cargo.lock must pin NSH to exactly one main commit, found: {sorted(commits) or 'none'}")
    return commits.pop()


def git(*args: str, capture: bool = False) -> str:
    result = subprocess.run(["git", "-C", str(SOURCE), *args], check=True, text=True, capture_output=capture)
    return result.stdout.strip() if capture else ""


def executable() -> Path:
    return SOURCE / "target/release" / ("nsh.exe" if sys.platform == "win32" else "nsh")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.parse_args()
    commit = pinned_commit()
    if SOURCE.exists():
        if not (SOURCE / ".git").exists():
            raise SystemExit(f"{SOURCE} is not an NSH checkout; remove it and run again")
        if git("remote", "get-url", "origin", capture=True).removesuffix(".git") != REPOSITORY:
            raise SystemExit(f"{SOURCE} does not use the NSH repository")
        if git("status", "--porcelain", capture=True):
            raise SystemExit(f"{SOURCE} has local changes; NSH is not modified here (AGENTS.md)")
    else:
        SOURCE.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", "-q", str(SOURCE)], check=True)
        git("remote", "add", "origin", REPOSITORY + ".git")
    current = git("rev-parse", "HEAD", capture=True) if _has_head() else ""
    if current != commit:
        print(f"Fetching NSH {commit[:7]}…", flush=True)
        git("fetch", "-q", "--depth", "1", "origin", commit)
        git("checkout", "-q", "--detach", commit)
    nsh = executable()
    if current == commit and nsh.is_file() and f"({commit[:7]})" in _version(nsh):
        print(f"{nsh}\n{_version(nsh)}")
        return
    # Official source build; --locked keeps NSH's own lock file.
    subprocess.run(["cargo", "build", "--release", "--locked", "-p", "nsh-cli"], cwd=SOURCE, check=True)
    version = _version(nsh)
    if f"({commit[:7]})" not in version:
        raise SystemExit(f"Built CLI reports '{version}', not commit {commit[:7]}")
    print(f"{nsh}\n{version}")


def _version(nsh: Path) -> str:
    return subprocess.check_output([str(nsh), "--version"], text=True).strip()


def _has_head() -> bool:
    return subprocess.run(["git", "-C", str(SOURCE), "rev-parse", "--verify", "-q", "HEAD"], capture_output=True).returncode == 0


if __name__ == "__main__":
    main()
