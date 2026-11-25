#!/usr/bin/env python3
"""Walks every page of the phone interface on a connected Android device or emulator and checks what it shows.

It drives the app through the accessibility tree (`android layout`) and `adb input`, the way a person would: onboarding,
adding accounts by setup key and by otpauth links, reordering by dragging, the details sheet, categories, search,
settings and their dialogs, export, locking, an import of many accounts (whose list it scrolls end to end), and erasing
the vault. Each check that fails is reported with a screenshot, and the run goes on.

It installs and runs the demo package (`python3 scripts/android.py --demo`, ID `com.gicek.safay.demo`), and it erases
that package's data at the start and the end. Never point it at a package that holds a vault you want to keep.

    python3 scripts/android_ui_test.py                  # build/android/demo/Safay Demo.apk on the only device
    python3 scripts/android_ui_test.py --serial emulator-5554 --no-install
    python3 scripts/android_ui_test.py --only categories,large-import

Needs `adb`, the `android` command-line tool (Android SDK cmdline-tools) and Cargo (the import's file comes from
`cargo run -p safay-core --example export_fixture`). Screenshots go to build/android/ui-test.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import hmac
import json
import os
import re
import shlex
import shutil
import struct
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = "com.gicek.safay.demo"
APK = ROOT / "build/android/demo/Safay Demo.apk"
ADB = shutil.which("adb") or str(Path.home() / "Library/Android/sdk/platform-tools/adb")

# Test data only: the vault lives in the demo package and is erased at the end.
PASSWORD = "Blue-Kettle-Orbit-42"
EXPORT_PASSWORD = "Export-Pass-2026"
GITHUB_SECRET = "JBSWY3DPEHPK3PXP"
DROPBOX_LINK = "otpauth://totp/Dropbox:bob@example.com?secret=GEZDGNBVGY3TQOJQ&issuer=Dropbox"
AWS_LINK = "otpauth://totp/AWS:ops?secret=MFRGGZDFMZTWQ2LK&issuer=AWS&digits=8&period=60&algorithm=SHA256"

# The Simplified Chinese language's own name, as the language dialog lists it, and the CJK ideographs.
SIMPLIFIED_CHINESE = "\u7b80\u4f53\u4e2d\u6587"
CJK = re.compile("[\u4e00-\u9fff]")
# Words an untranslated Chinese screen may still show: names of services, accounts and the app.
NAMES = {"Safay", "Dropbox", "OneDrive", "Google Drive", "Nextcloud", "Synology", "WebDAV", "GitHub", "AWS", "English"}


# ---- Device ----------------------------------------------------------------------------------------------------------


@dataclass
class Node:
    label: str  # what screen readers say: its text or its description
    rid: str  # its test ID (`resource-id`)
    cls: str
    left: int
    top: int
    right: int
    bottom: int
    clickable: bool

    @property
    def center(self) -> tuple[int, int]:
        return (self.left + self.right) // 2, (self.top + self.bottom) // 2


@dataclass
class Screen:
    nodes: list[Node]
    status_bottom: int
    keyboard_top: int | None
    height: int

    def find(self, label: str, exact: bool = True) -> list[Node]:
        """Nodes whose label or test ID is `label` (or, not `exact`, whose label contains it)."""
        if exact:
            return [n for n in self.nodes if label in (n.label, n.rid)]
        return [n for n in self.nodes if label in n.label]

    def has(self, label: str, exact: bool = True) -> bool:
        return bool(self.find(label, exact))

    def visible(self, node: Node) -> bool:
        """Whole between the status bar and the keyboard (or the screen's bottom)."""
        bottom = self.keyboard_top if self.keyboard_top is not None else self.height
        return node.top >= self.status_bottom and node.bottom <= bottom

    def labels(self) -> list[str]:
        return [n.label for n in self.nodes if n.label]

    def top_of(self, label: str) -> int:
        n = self.find(label)
        return n[0].top if n else 0


@dataclass
class Capture:
    width: int
    height: int
    data: bytes
    offset: int

    def pixel(self, x: int, y: int) -> tuple[int, int, int]:
        at = self.offset + (y * self.width + x) * 4
        return self.data[at], self.data[at + 1], self.data[at + 2]

    def column(self, x: int) -> bytes:
        return bytes(b for y in range(self.height) for b in self.pixel(x, y))


class Device:
    def __init__(self, serial: str, shots: Path):
        self.serial = serial
        self.shots = shots

    def adb(self, *args: str, check: bool = False) -> subprocess.CompletedProcess:
        return subprocess.run([ADB, "-s", self.serial, *args], capture_output=True, check=check)

    def shell(self, command: str) -> str:
        return self.adb("shell", command).stdout.decode(errors="replace").strip()

    def screen(self) -> Screen:
        # A page that never settles (the camera's picture) is read without waiting for the layout to be idle.
        for idle in (True, True, False, False):
            command = ["android", "layout", f"--device={self.serial}", "--flat"] + ([] if idle else ["--no-idle"])
            out = subprocess.run(command, capture_output=True, text=True).stdout
            if "[" in out:
                break
            time.sleep(0.5)
        windows = json.loads(out[out.index("[") :])
        nodes: list[Node] = []
        status_bottom, keyboard_top, height = 0, None, 0
        for w in windows:
            b = [int(v) for v in re.findall(r"-?\d+", w.get("bounds", "[0,0][0,0]"))]
            kind = w.get("type")
            if kind == "SYSTEM" and b[1] == 0:
                status_bottom = max(status_bottom, b[3])
            elif kind == "INPUT_METHOD":
                keyboard_top = b[1]
            elif kind == "APPLICATION":
                height = max(height, b[3])
                for n in w.get("content", []):
                    nb = [int(v) for v in re.findall(r"-?\d+", n["bounds"])]
                    nodes.append(
                        Node(
                            label=n.get("text") or n.get("content-desc") or "",
                            rid=n.get("resource-id") or "",
                            cls=n.get("class", "").rsplit(".", 1)[-1],
                            left=nb[0],
                            top=nb[1],
                            right=nb[2],
                            bottom=nb[3],
                            clickable="CLICKABLE" in n.get("interactions", []),
                        )
                    )
        return Screen(nodes, status_bottom, keyboard_top, height)

    def wait(self, label: str, timeout: float = 6.0, exact: bool = True, gone: bool = False) -> Screen:
        end = time.monotonic() + timeout
        while True:
            s = self.screen()
            if s.has(label, exact) != gone or time.monotonic() > end:
                return s
            time.sleep(0.4)

    def node(self, label: str, index: int = 0, exact: bool = True, timeout: float = 6.0) -> Node:
        s = self.wait(label, timeout, exact)
        found = s.find(label, exact)
        if len(found) <= index:
            raise StepError(f"no element {label!r} on screen")
        return found[index]

    def tap(self, label: str, index: int = 0, exact: bool = True, timeout: float = 6.0, settle: float = 0.8):
        x, y = self.node(label, index, exact, timeout).center
        self.adb("shell", "input", "tap", str(x), str(y))
        time.sleep(settle)

    def tap_id(self, rid: str, settle: float = 0.8):
        """Taps the element whose test ID is `rid`, where its label is on another element too."""
        found = [n for n in self.wait(rid).nodes if n.rid == rid]
        if not found:
            raise StepError(f"no element with the test ID {rid!r} on screen")
        x, y = found[0].center
        self.adb("shell", "input", "tap", str(x), str(y))
        time.sleep(settle)

    def long_press(self, label: str, settle: float = 1.0):
        x, y = self.node(label).center
        self.adb("shell", "input", "swipe", str(x), str(y), str(x), str(y), "800")
        time.sleep(settle)

    def drag(self, node: Node, to_y: int, hold: float = 0.9):
        """Touches and holds `node`, moves the finger to `to_y` in small steps, and lifts it: a long press, then a drag.
        (`input swipe` moves at once, so it never makes a long press.)"""
        x, y = node.center
        self.shell(f"input motionevent DOWN {x} {y}")
        time.sleep(hold)
        for step in range(1, 11):
            self.shell(f"input motionevent MOVE {x} {y + (to_y - y) * step // 10}")
            time.sleep(0.06)
        time.sleep(0.4)
        self.shell(f"input motionevent UP {x} {to_y}")
        time.sleep(2.0)

    def swipe(self, x1: int, y1: int, x2: int, y2: int, ms: int = 300):
        self.adb("shell", "input", "swipe", str(x1), str(y1), str(x2), str(y2), str(ms))
        time.sleep(0.8)

    def type(self, text: str, settle: float = 0.5):
        """Types into the focused field. Spaces go as %s, everything else quoted for the device's shell."""
        self.adb("shell", "input text " + shlex.quote(text.replace(" ", "%s")))
        time.sleep(settle)

    def key(self, code: int | str, settle: float = 0.3):
        self.adb("shell", "input", "keyevent", str(code))
        time.sleep(settle)

    def fill(self, field_label: str, text: str, index: int = 1):
        """Focuses the field labeled `field_label` (index 1: the EditText under its floating label) and types."""
        self.tap(field_label, index, settle=0.5)
        self.type(text)

    def hide_keyboard(self):
        if self.screen().keyboard_top is not None:
            self.key(4)

    def focused_window(self) -> str:
        for line in self.shell("dumpsys window").splitlines():
            if "mCurrentFocus=Window" in line:
                return line.strip()
        return ""

    def capture(self) -> Capture:
        """The screen's pixels, from a raw capture (RGBA after a header of 12 or 16 bytes)."""
        data = self.adb("exec-out", "screencap").stdout
        w, h = struct.unpack("<II", data[:8])
        return Capture(w, h, data, len(data) - w * h * 4)

    def dialog_clipped(self) -> bool:
        """A dialog runs under the status bar: just below it, a point inside the panel (110 px in, past its 27 pt side
        margin) and one in that margin (40 px in, past a bottom sheet's rounded corner under it) differ. (Its
        accessibility bounds do not show this: they are where the panel was before it moved.)"""
        y = self.screen().status_bottom + 6
        cap = self.capture()
        return distance(cap.pixel(40, y), cap.pixel(110, y)) > 36

    def epoch(self) -> int:
        return int(self.shell("date +%s"))

    def shot(self, name: str) -> Path:
        path = self.shots / f"{name}.png"
        path.write_bytes(self.adb("exec-out", "screencap", "-p").stdout)
        return path

    def launch(self):
        # The launcher activity, started by name: `monkey` exits at once on some system images (Android 12's emulator).
        activity = self.shell(f"cmd package resolve-activity --brief -c android.intent.category.LAUNCHER {PACKAGE}").splitlines()[-1].strip()
        self.shell(f"am start -W -n {activity}")
        time.sleep(2.5)


def settled(d: Device, x: int = 60) -> tuple[Screen, Capture]:
    """The layout and the pixels of a list that has stopped moving: the column at `x` (the rows' edge, where nothing
    animates) is the same before and after the layout is read, so the two describe the same frame."""
    for _ in range(5):
        before = d.capture()
        s = d.screen()
        after = d.capture()
        if before.column(x) == after.column(x):
            return s, after
        time.sleep(0.5)
    return s, after


def distance(a: tuple[int, int, int], b: tuple[int, int, int]) -> int:
    return sum(abs(x - y) for x, y in zip(a, b, strict=True))


# ---- Checks ----------------------------------------------------------------------------------------------------------


class StepError(Exception):
    pass


@dataclass
class Result:
    step: str
    failures: list[str] = field(default_factory=list)
    error: str | None = None


class Run:
    def __init__(self, device: Device):
        self.d = device
        self.results: list[Result] = []
        self.current: Result | None = None
        self.count = 0

    def check(self, ok: bool, what: str):
        assert self.current is not None
        mark = "ok  " if ok else "FAIL"
        print(f"    {mark} {what}")
        if not ok:
            self.count += 1
            shot = self.d.shot(f"fail-{self.count:02d}-{self.current.step}")
            self.current.failures.append(f"{what} ({rel(shot)})")

    def step(self, name: str, fn):
        print(f"\n== {name}")
        self.current = Result(name)
        self.results.append(self.current)
        try:
            fn(self)
        except Exception as e:  # a step that cannot go on: note it, put the app back on the list, go on
            self.current.error = f"{type(e).__name__}: {e}"
            shot = self.d.shot(f"error-{name}")
            print(f"    ERROR {self.current.error} ({rel(shot)})")
            recover(self.d)

    def report(self) -> int:
        failed = [r for r in self.results if r.failures or r.error]
        print("\n" + "=" * 100)
        for r in self.results:
            state = "ERROR" if r.error else "FAIL" if r.failures else "pass"
            print(f"{state:<6}{r.step}")
            for f in r.failures:
                print(f"        - {f}")
            if r.error:
                print(f"        - {r.error}")
        print(f"\n{len(self.results) - len(failed)} of {len(self.results)} steps passed. Screenshots: {self.d.shots}")
        return 1 if failed else 0


def rel(path: Path) -> str:
    return str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path)


def totp(secret: str, at: int, digits: int = 6, period: int = 30) -> str:
    key = base64.b32decode(secret.upper() + "=" * (-len(secret) % 8))
    h = hmac.new(key, struct.pack(">Q", at // period), hashlib.sha1).digest()
    o = h[-1] & 15
    return str((struct.unpack(">I", h[o : o + 4])[0] & 0x7FFFFFFF) % 10**digits).zfill(digits)


ROW = "Copy code for "  # how screen readers name a row of the list, before its account's name


def is_row(n: Node) -> bool:
    return n.label.startswith(ROW)


def row_name(n: Node) -> str:
    """The account a row shows; empty for a row with nothing in it."""
    return n.label[len(ROW) :]


def rows(s: Screen) -> list[str]:
    return [row_name(n) for n in s.nodes if is_row(n)]


def recover(d: Device):
    """Back to the list, whatever is open: system back until the list's top bar shows, with no search open."""
    d.hide_keyboard()
    for _ in range(6):
        s = d.screen()
        if s.has("more") and not s.has("Back") and not s.has("Close"):
            return
        if PACKAGE not in d.focused_window():
            d.launch()
            continue
        d.key(4, settle=0.8)


def open_settings(d: Device):
    d.tap("More")
    d.tap("Settings")
    d.wait("Security")


def scroll_down(d: Device):
    d.swipe(540, 1800, 540, 700)


def scroll_up(d: Device):
    d.swipe(540, 700, 540, 1900)


def reveal(d: Device, *labels: str, tries: int = 4) -> Screen:
    """Scrolls down until every one of `labels` is on screen (a shorter screen shows less of a page), at most `tries`
    times; the last screen either way."""
    s = d.screen()
    for _ in range(tries):
        if all(s.has(label) for label in labels):
            break
        scroll_down(d)
        time.sleep(0.6)
        s = d.screen()
    return s


# ---- Steps -----------------------------------------------------------------------------------------------------------


def onboarding(r: Run):
    d = r.d
    s = d.wait("Start fresh")
    r.check(s.has("Start fresh") and s.has("Restore a backup"), 'welcome shows "Start fresh" and "Restore a backup"')
    d.tap("Restore a backup")
    s = d.wait("Safay Exported File")
    r.check(
        s.has("Safay Exported File") and s.has("WebDAV"), "restore lists the backup locations and the exported file"
    )
    d.key(4, settle=1.0)
    d.tap("Start fresh")
    d.wait("Set master password")
    d.fill("New Password", "testpass1")
    s = d.wait("Too easy", exact=False)
    r.check(s.has("Too easy", exact=False), "a weak password is called out")
    d.tap("New Password", 1)
    d.key("KEYCODE_MOVE_END")
    for _ in range(12):
        d.key(67, settle=0.05)
    d.type(PASSWORD)
    d.fill("Enter again", PASSWORD[:-1] + "x")
    s = d.wait("Passwords do not match")
    r.check(s.has("Passwords do not match"), "a mismatch is called out")
    d.key(67)
    d.type(PASSWORD[-1])
    s = d.wait("Passwords do not match", gone=True)
    r.check(not s.has("Passwords do not match"), "the mismatch note goes once both match")
    d.key(66, settle=3.0)  # the keyboard's Go key creates the vault
    s = d.wait("No 2FA accounts yet", timeout=10)
    r.check(s.has("No 2FA accounts yet"), "the empty list shows after creating the vault")


def settings_biometrics_off(r: Run):
    """Biometrics are on after creating a vault, and the system prompt needs a person; the rest of the run uses the
    master password."""
    d = r.d
    open_settings(d)
    if d.screen().has("biometrics"):  # a device with no biometrics enrolled has no such row
        d.tap("biometrics", settle=1.5)
    d.tap("Back")


def scan(r: Run):
    """The camera page opens from the add menu (the permission granted at the start) and closes back to the list."""
    d = r.d
    d.tap("Add Account", settle=1.0)
    s = d.screen()
    menu = list(dict.fromkeys(n.label for n in s.nodes if n.label in ("Scan QR Code", "Choose from Photos")))
    r.check(menu[:1] == ["Scan QR Code"], f"scanning is the add menu's first choice ({menu})")
    d.tap_id("Scan QR Code")
    s = d.wait("Point the camera at the QR code", timeout=8)
    r.check(s.has("Point the camera at the QR code"), "the camera page opens, aiming")
    d.tap("scan-close", settle=1.2)
    r.check(d.screen().has("more"), "closing the camera page goes back to the list")


def manual_entry(r: Run):
    d = r.d
    d.tap("Add Account")
    s = d.wait("Choose from Photos")
    r.check(
        all(s.has(x) for x in ("Choose from Photos", "Enter Setup Key", "Import Backup File")),
        "the add menu shows its three choices",
    )
    d.tap_id("Enter Setup Key")  # the add menu's, not the empty list's button of the same name
    d.wait("Manual Entry")
    s = d.screen()
    for tab in ("Manual Entry", "Paste otpauth"):
        r.check(any(n.clickable and n.label == tab for n in s.nodes), f'the "{tab}" tab is exposed as a control')
    d.fill("Service Name", "GitHub")
    d.fill("Account / Email", "alice@example.com")
    d.fill("Secret", GITHUB_SECRET)
    s = d.screen()
    typed = [n.label for n in s.nodes if n.cls == "EditText"]
    r.check(GITHUB_SECRET in typed, f"the Secret field keeps every typed character (has {typed[-1:]!r})")
    if GITHUB_SECRET not in typed:  # type it again slowly so the rest of the run has the account
        d.tap("Secret", 1)
        for _ in range(24):
            d.key(67, settle=0.03)
        for c in GITHUB_SECRET:
            d.type(c, settle=0.15)
    d.hide_keyboard()
    d.tap("advanced")
    s = d.wait("Algorithm")
    r.check(all(s.has(x) for x in ("SHA1", "SHA256", "SHA512")), "advanced options open")
    s = reveal(d, "30 seconds", "60 seconds")
    r.check(s.has("30 seconds") and s.has("60 seconds"), "advanced options list the refresh intervals")
    reveal(d, "save")
    d.tap("save", settle=2.0)
    s = d.wait("Copy code for GitHub")
    r.check(rows(s) == ["GitHub"], f"the list has the new account (rows {rows(s)})")
    at = d.epoch()
    want = totp(GITHUB_SECRET, at)
    codes = [n.label.replace(" ", "") for n in s.nodes if re.fullmatch(r"\d{3} \d{3}", n.label)]
    r.check(want in codes or totp(GITHUB_SECRET, at - 1) in codes, f"the code is right (want {want}, shows {codes})")


def paste_links(r: Run):
    d = r.d
    d.tap("Add Account")
    d.tap("Enter Setup Key", 0)
    d.tap("Paste otpauth")
    d.tap("otpauth")
    d.type(DROPBOX_LINK)
    d.key(66)
    d.type(AWS_LINK)
    d.hide_keyboard()
    d.tap("parse", settle=1.5)
    s = d.wait("add-all")
    r.check(s.has("Dropbox") and s.has("AWS") and s.has("8 digits · 60 sec"), "review shows both links' accounts")
    r.check(s.has("Add 2 Accounts"), 'the button reads "Add 2 Accounts"')
    d.tap("add-all", settle=2.0)
    s = d.wait("Copy code for AWS")
    r.check(sorted(rows(s)) == ["AWS", "Dropbox", "GitHub"], f"three accounts on the list (rows {rows(s)})")
    rings = {n.label for n in s.nodes if re.fullmatch(r"\d{1,2}", n.label)}
    at = d.epoch()
    r.check(any(abs(int(x) - (60 - at % 60)) <= 2 for x in rings), "the 60-second account counts down from 60")


def reorder_accounts(r: Run):
    d = r.d
    s = d.screen()
    before = rows(s)
    shown = [n for n in s.nodes if is_row(n)]
    d.drag(shown[-1], shown[0].center[1])
    s = d.screen()
    want = [before[-1], *before[:-1]]
    r.check(rows(s) == want, f"a long press and a drag move an account to the top (rows {rows(s)}, want {want})")
    r.check(not s.has("Details"), "the account's menu closes once the drag starts")
    d.drag([n for n in s.nodes if is_row(n)][0], shown[-1].center[1])
    s = d.screen()
    r.check(rows(s) == before, f"and back to the end (rows {rows(s)})")


def details(r: Run):
    d = r.d
    d.long_press("Copy code for GitHub")
    s = d.screen()
    # Each item is its button and its text: their labels once each, in order.
    menu = list(
        dict.fromkeys(n.label for n in s.nodes if n.label in ("Show QR Code", "Details", "Add to", "Delete", "Copy"))
    )
    r.check(menu == ["Show QR Code", "Details", "Add to", "Delete"], f"a long press opens the account menu ({menu})")
    d.tap("Details", settle=1.2)
    s = d.wait("Show Secret")
    r.check(s.has("Show Secret") and s.has("Export QR Code"), "the details sheet opens")
    r.check(not s.has("delete"), "the details sheet has no delete button")
    d.tap("secret", settle=1.2)
    s = d.wait("Verify your identity")
    r.check(s.has("Verify your identity"), "showing the secret asks for the master password")
    if s.has("Verify your identity"):
        d.fill("Master Password", PASSWORD)
        r.check(not d.dialog_clipped(), "the verify dialog stays below the status bar with the keyboard up")
        d.key(66, settle=2.0)
        s = d.wait(GITHUB_SECRET)
        r.check(s.has(GITHUB_SECRET), "the secret shows after the password")
        d.tap("secret")  # Hide
    d.tap("qr", settle=1.0)
    s = d.wait("Verify your identity", timeout=3)
    if s.has("Verify your identity"):
        d.fill("Master Password", PASSWORD)
        d.key(66, settle=2.0)
    s = d.wait("qr-done")
    r.check(s.has("qr-done") and s.has("Anyone who scans", exact=False), "the QR code dialog opens")
    d.tap("qr-done")
    d.tap("Edit", settle=1.0)
    d.fill("Website", "github.com")
    d.tap("Service Name", 1)
    d.key("KEYCODE_MOVE_END")
    d.type(" Work")
    d.hide_keyboard()
    d.tap("done", settle=2.0)
    recover(d)
    s = d.wait("Copy code for GitHub Work")
    r.check("GitHub Work" in rows(s), f"the edited name shows on the list (rows {rows(s)})")


def categories(r: Run):
    d = r.d
    d.long_press("Copy code for AWS")
    d.tap("Add to", settle=1.2)
    d.tap("New Category", settle=1.0)
    d.fill("Category name", "Work")
    r.check(not d.dialog_clipped(), "the New Category dialog stays below the status bar with the keyboard up")
    d.tap("commit", settle=1.0)
    s = d.wait("Work")
    r.check(s.has("Work"), "the new category is in the sheet")
    d.tap("save", settle=1.0)
    d.tap("Work", settle=1.5)
    s = d.screen()
    r.check(rows(s) == ["AWS"], f'the "Work" chip shows only AWS (rows {rows(s)})')
    d.tap("All", settle=1.0)
    s = d.screen()
    names = sorted(x.removesuffix(" Work") for x in rows(s))  # "GitHub Work" once the details step renamed it
    r.check(names == ["AWS", "Dropbox", "GitHub"], f'"All" shows every account (rows {rows(s)})')

    d.tap("manage", settle=1.2)
    d.wait("Manage Categories")
    d.tap("new-category")
    d.fill("Category name", "Personal")
    d.tap("commit", settle=1.0)
    d.tap("category", 0)
    d.tap("Category name", 1)
    d.key("KEYCODE_MOVE_END")
    d.type(" Team")
    d.key(66, settle=1.0)
    s = d.screen()
    r.check(s.has("Work Team"), "a category is renamed")
    r.check(not s.has("Move Up") and not s.has("Move Down"), "categories have no move buttons")
    shown = [n for n in s.nodes if n.rid == "category"]
    d.drag(shown[1], shown[0].center[1])
    s = d.screen()
    names = [n.label for n in s.nodes if n.label in ("Work Team", "Personal")]
    r.check(names == ["Personal", "Work Team"], f"a long press and a drag reorder the categories ({names})")
    r.check(not s.has("Rename"), "dragging a category does not rename it")
    d.tap("Delete Category", 0)
    d.tap("Delete Category", 0, settle=1.0)  # the confirmation's button
    s = d.screen()
    r.check(not s.has("Personal") and s.has("Work Team"), "a category is deleted")
    d.tap("Back")


def search(r: Run):
    d = r.d
    d.tap("search-button", settle=1.2)
    s = d.screen()
    r.check(s.keyboard_top is not None, "the search button opens the field with the keyboard up")
    d.type("drop")
    s = d.wait("Close")
    r.check(rows(s) == ["Dropbox"], f"search finds Dropbox (rows {rows(s)})")
    for _ in range(4):
        d.key(67, settle=0.05)
    d.type("zzz")
    s = d.wait("No matching accounts", timeout=3)
    n = s.find("No matching accounts")
    r.check(bool(n) and s.visible(n[0]), '"No matching accounts" shows above the keyboard')
    d.tap("search-button", settle=1.2)
    s = d.screen()
    r.check(s.keyboard_top is None and not s.has("Close"), "the close button closes the field and the keyboard")
    r.check(len(rows(s)) == 3, "closing the search brings every account back")


def delete_and_recover(r: Run):
    d = r.d
    d.long_press("Copy code for Dropbox")
    d.tap("Delete", settle=1.5)
    s = d.screen()
    r.check(
        sorted(rows(s)) == ["AWS", "GitHub Work"],
        f"the list drops the deleted account and keeps the others (rows {rows(s)})",
    )
    open_settings(d)
    scroll_down(d)
    d.tap("deleted", settle=1.2)
    s = d.wait("Recover")
    r.check(s.has("Dropbox") and s.has("Accounts are deleted for good after 30 days."), "Recently Deleted lists it")
    d.tap("Recover", settle=1.2)
    s = d.screen()
    r.check(s.has("No recently deleted accounts"), "recovering empties Recently Deleted")
    recover(d)
    s = d.screen()
    r.check(sorted(rows(s)) == ["AWS", "Dropbox", "GitHub Work"], f"the account is back (rows {rows(s)})")


def settings_dialogs(r: Run):
    d = r.d
    open_settings(d)
    d.tap("auto-lock")
    s = d.wait("1 minute")
    r.check(all(s.has(x) for x in ("1 minute", "5 minutes", "15 minutes", "30 minutes")), "Auto-Lock lists its times")
    d.tap("15 minutes", settle=1.0)
    r.check(d.screen().has("15 minutes"), "the chosen time shows under Auto-Lock")
    d.tap("auto-lock")
    d.tap("5 minutes", settle=1.0)

    d.tap("change-password", settle=1.0)
    d.fill("Current Master Password", PASSWORD)
    r.check(not d.dialog_clipped(), "the Change Master Password dialog and its focused field stay on screen")
    d.hide_keyboard()
    d.tap("Cancel")

    scroll_down(d)
    d.tap("reset", settle=1.0)
    s = d.wait("Erase Local Data")
    r.check(s.has("Reset Safay?"), "Reset App asks first")
    d.tap("Cancel")

    d.tap("Dark", settle=1.0)
    d.shot("settings-dark")
    d.tap("System", 0, settle=1.0)

    scroll_up(d)
    d.tap("backup-set-up", settle=1.2)
    s = d.wait("Choose Location")
    r.check(
        all(s.has(x) for x in ("Dropbox", "OneDrive", "Google Drive", "Nextcloud", "Synology", "WebDAV")),
        "Choose Location lists the services",
    )
    d.tap("WebDAV", 0, settle=1.2)
    d.wait("Connect to WebDAV")
    d.fill("Server Address", "https://example.com/dav/")
    s = d.screen()
    t = s.find("Connect to WebDAV")
    r.check(bool(t) and s.visible(t[0]), "the WebDAV sheet stays on screen above the keyboard")
    d.hide_keyboard()
    d.tap("Close")

    # Synology: the NAS, the account and an optional code; the folder is picked after signing in. Closing a sheet goes
    # back to Settings, so the locations are opened again.
    d.tap("backup-set-up", settle=1.2)
    d.wait("Choose Location")
    d.tap("Synology", 0, settle=1.2)
    s = d.wait("Connect to Synology")
    r.check(
        all(s.has(x) for x in ("QuickConnect ID or Address", "Username", "Password", "Verification Code (Optional)"))
        and not s.has("Shared Folder"),
        "the Synology sheet asks for the NAS, the account and an optional code",
    )
    d.fill("QuickConnect ID or Address", "my-nas")
    s = d.screen()
    t = s.find("Connect to Synology")
    r.check(bool(t) and s.visible(t[0]), "the Synology sheet stays on screen above the keyboard")
    d.hide_keyboard()
    d.shot("backup-synology")
    d.tap("Close")
    d.tap("Back")


def language(r: Run):
    d = r.d
    open_settings(d)
    scroll_down(d)
    d.tap("language")
    d.tap(SIMPLIFIED_CHINESE, settle=1.5)
    seen: set[str] = set()
    scroll_up(d)
    for _ in range(2):
        seen.update(d.screen().labels())
        scroll_down(d)
    seen.update(d.screen().labels())
    english = sorted(
        x
        for x in seen
        if re.search(r"[A-Za-z]{3}", x)
        and not CJK.search(x)
        and x not in NAMES
        and not re.fullmatch(r"[a-z0-9-]+", x)
        and not re.fullmatch(r"[\d. ()]+", x)
        and x not in ("Show password", "Back")  # NSH's own accessibility labels
    )
    r.check(not english, f"settings in Simplified Chinese show no English ({english})")
    d.shot("settings-zh-Hans")
    d.tap("language")
    d.tap("English", settle=1.5)
    scroll_up(d)
    d.tap("Back")


def export(r: Run):
    d = r.d
    open_settings(d)
    scroll_down(d)
    d.tap("export", settle=1.0)
    d.fill("Export Password", EXPORT_PASSWORD)
    d.fill("Enter again", EXPORT_PASSWORD)
    d.key(66, settle=2.0)
    s = d.wait("Verify your identity")
    r.check(s.has("Verify your identity"), "export asks for the master password")
    d.fill("Master Password", PASSWORD)
    d.key(66, settle=3.0)
    window = d.focused_window()
    r.check("intentresolver" in window or "Chooser" in window, f"the share sheet opens with the file ({window})")
    d.key(4, settle=1.5)
    recover(d)


def lock(r: Run):
    d = r.d
    d.tap("More")
    d.tap("Lock Now", settle=1.5)
    s = d.wait("Safay is locked")
    r.check(s.has("Safay is locked"), "Lock Now locks")
    r.check(s.has("Use your master password to unlock") and s.has("unlock"), "it says what unlocks it, with Enter's arrow")
    d.shot("lock")
    # The field has no label on screen: it is found by its ID.
    d.tap("password", settle=0.5)
    d.type("wrong-password")
    d.key(66, settle=2.0)
    s = d.screen()
    r.check(
        s.has("Incorrect master password", exact=False) or s.has("incorrect", exact=False),
        "a wrong password is called out",
    )
    d.tap("password", settle=0.5)
    d.type(PASSWORD)
    d.tap("unlock", settle=2.5)
    s = d.wait(ROW, exact=False)
    r.check(len(rows(s)) == 3, "the right password, and the arrow, unlock to the list")


def back_navigation(r: Run):
    d = r.d
    open_settings(d)
    d.key(4, settle=1.0)
    s = d.screen()
    r.check(s.has("more") and not s.has("Security"), "system back leaves Settings for the list")
    d.tap("Add Account")
    d.key(4, settle=0.8)
    r.check(not d.screen().has("Choose from Photos"), "system back closes the add menu")


# The list of many accounts: an export of LARGE accounts made by `export_fixture`, imported from the device's Downloads.
LARGE = 150
LARGE_FILE = f"safay-ui-test-{LARGE}.json"
FIXTURE = re.compile(r"^[A-Za-z]+ (\d{3})$")


def row_corners(cap: Capture, node: Node) -> tuple[bool, bool]:
    """Whether the row's top and bottom corners are the group's outer ones (24 pt) rather than the inner ones (6 pt):
    10 px in from a corner is outside a 24 pt curve (the page's background shows) and inside a 6 pt one (the row's)."""
    mid = (node.top + node.bottom) // 2
    card, page = cap.pixel(node.left + 12, mid), cap.pixel(max(node.left - 12, 0), mid)

    def outer(y: int) -> bool:
        p = cap.pixel(node.left + 10, y)
        return distance(p, page) < distance(p, card)

    return outer(node.top + 10), outer(node.bottom - 11)


def whole_rows(s: Screen, bottom_limit: int) -> list[Node]:
    """The list's rows shown whole: below the chips, above the add button's level, and as tall as a row (the
    accessibility bounds of a row the list's edge cuts are cut too)."""
    top_limit = s.top_of("All") + 60
    rows_ = [n for n in s.nodes if is_row(n) and n.top >= top_limit and n.bottom <= bottom_limit]
    tallest = max((n.bottom - n.top for n in rows_), default=0)
    return [n for n in rows_ if n.bottom - n.top >= tallest * 0.95]


def walk_list(r: Run, d: Device, label: str) -> list[str]:
    """Scrolls the list from its top to its end and back, checking every row it shows: a name, no row twice, and
    outer corners only at the top of the first row and the bottom of the last. Gives the names in order."""
    for _ in range(3):  # to the top
        d.swipe(540, 700, 540, 2100, 120)
    order: list[str] = []
    corners: dict[str, set[tuple[bool, bool]]] = {}
    evidence: dict[str, str] = {}  # a screenshot of each row seen with an outer corner, and where the row was
    blank = 0
    still = 0
    bottom_limit = 2250
    for _ in range(80):
        s, cap = settled(d)
        shown = whole_rows(s, bottom_limit)
        names = [row_name(n) for n in shown]
        blank += sum(1 for x in names if not x)
        if len(names) != len(set(names)):
            r.check(False, f"{label}: no row shows twice on one screen ({names})")
        for n, name in zip(shown, names, strict=True):
            seen = row_corners(cap, n)
            corners.setdefault(name, set()).add(seen)
            if any(seen) and name not in evidence:
                shot = d.shot(f"corners-{label}-{len(evidence):02d}")
                evidence[name] = f"{rel(shot)} rows {n.top}-{n.bottom}"
        before = len(order)
        for name in names:
            if name and name not in order:
                order.append(name)
        if len(order) == before:
            still += 1
            if still >= 2:
                break
        else:
            still = 0
        d.swipe(540, 1900, 540, 900, 900)
    r.check(blank == 0, f"{label}: every row shows its account ({blank} empty rows seen)")
    first, last = (order[0], order[-1]) if order else ("", "")
    wrong = sorted(
        f"{name} {sorted(seen)} {evidence.get(name, '')}"
        for name, seen in corners.items()
        if name and seen != {(name == first, name == last)}
    )
    r.check(not wrong, f"{label}: outer corners only at the list's ends, steady while it scrolls ({wrong[:6]})")
    # Flings both ways, then the screen once more: fast scrolling must leave nothing stale.
    for _ in range(3):
        d.swipe(540, 2000, 540, 500, 80)
        d.swipe(540, 600, 540, 1800, 80)
    time.sleep(1.5)
    s, cap = settled(d)
    shown = whole_rows(s, bottom_limit)
    names = [row_name(n) for n in shown]
    ok = all(names) and len(names) == len(set(names))
    ok = ok and all(row_corners(cap, n) == (name == first, name == last) for n, name in zip(shown, names, strict=True))
    r.check(ok, f"{label}: after flinging, the rows show their accounts with the right corners ({names})")
    return order


def letter_filter(r: Run, d: Device):
    """The letter index lists the names' first letters; a press on one narrows the list to the accounts filed under
    it, and each way out lets the list go: the letter again, a category, the search, the back key."""

    def letter(name: str) -> Node | None:
        return next((n for n in d.screen().nodes if n.label == name and n.right > 1000), None)

    def press(name: str):
        node = letter(name)
        if node is None:
            raise StepError(f"the index has no {name}")
        x, y = node.center
        d.shell(f"input motionevent DOWN {x} {y}")
        time.sleep(0.15)
        d.shell(f"input motionevent UP {x} {y}")
        time.sleep(1.2)

    def narrowed_to_v() -> bool:
        names = rows(d.screen())
        return bool(names) and all(name.startswith("Vercel") for name in names)

    def whole() -> bool:
        return next((name for name in rows(d.screen()) if name), "") == "GitHub 001"

    d.swipe(540, 700, 540, 2100, 120)
    s = d.screen()
    letters = [
        n.label for n in s.nodes if len(n.label) == 1 and (n.label.isupper() or n.label == "#") and n.right > 1000
    ]
    want = list("ACDFGHLMNPRSTV")  # the first letters of export_fixture's services
    r.check(letters == want, f"the index lists the letters of the names ({''.join(letters)})")
    press("V")
    r.check(narrowed_to_v(), f"a press on V narrows the list to the accounts under V ({rows(d.screen())})")
    press("V")
    r.check(whole(), "a press on V again lets the list go")
    press("V")
    d.tap("All", settle=1.2)
    r.check(whole(), '"All" lets the list go')
    press("V")
    d.key(4, settle=1.2)
    s = d.screen()
    r.check(whole() and s.has("more"), "the back key lets the list go, and stays in the app")
    press("V")
    d.tap("search-button", settle=1.2)
    r.check(whole(), "opening the search lets the list go")
    d.tap("search-button", settle=1.2)
    press("V")
    d.tap("Work", settle=1.5)
    work = [int(m.group(1)) for x in rows(d.screen()) if (m := FIXTURE.match(x))]
    r.check(bool(work) and all(n % 3 == 1 for n in work), f"a category lets the letter go and shows itself ({work})")
    d.tap("All", settle=1.2)
    # Sliding from A to T narrows the list to each letter in turn, ending at T.
    a, z = letter("A"), letter("T")
    if a and z:
        x = a.center[0]
        d.shell(f"input motionevent DOWN {x} {a.center[1]}")
        for step in range(1, 11):
            d.shell(f"input motionevent MOVE {x} {a.center[1] + (z.center[1] - a.center[1]) * step // 10}")
            time.sleep(0.05)
        d.shell(f"input motionevent UP {x} {z.center[1]}")
        time.sleep(1.2)
        names = rows(d.screen())
        r.check(
            bool(names) and all(n.startswith("Twitter") for n in names), f"sliding to T narrows the list to T ({names})"
        )
        press("T")


def large_import(r: Run):
    d = r.d
    local = d.shots / f"safay-{LARGE}.json"
    remote = f"/sdcard/Download/{LARGE_FILE}"
    made = subprocess.run(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            "safay-core",
            "--example",
            "export_fixture",
            "--",
            str(LARGE),
            EXPORT_PASSWORD,
            str(local),
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if made.returncode != 0:
        raise StepError(f"export_fixture failed: {made.stderr.strip()[-300:]}")
    d.adb("push", str(local), remote, check=True)
    try:
        d.tap("Add Account")
        d.tap("Import Backup File", 0, settle=2.5)
        # The system's file picker, which opens where it was last (a search bar on Recent, a search button in a folder)
        # and may cut a long name short: its search finds the file, by the start of its name.
        s = d.wait("Search", timeout=8, exact=False)
        d.tap("Search this device" if s.has("Search this device") else "Search", settle=1.0)
        d.type(LARGE_FILE)
        d.key(66, settle=2.5)
        start = "safay-ui-test-"
        listed = d.wait(start, timeout=10, exact=False).find(start, exact=False)
        found = [n for n in listed if n.cls == "TextView" and n.top > 300]
        if not found:
            raise StepError(f"the file picker does not list {LARGE_FILE}")
        x, y = found[0].center
        d.adb("shell", "input", "tap", str(x), str(y))
        time.sleep(2.0)
        s = d.wait("Backup Password")
        r.check(s.has("Backup Password"), "an encrypted export asks for its password")
        d.fill("Enter password", EXPORT_PASSWORD)
        r.check(not d.dialog_clipped(), "the Backup Password dialog stays below the status bar with the keyboard up")
        d.key(66, settle=1.0)
        s = d.wait("Backup Password", timeout=20, gone=True)
        r.check(not s.has("Backup Password"), "the password dialog closes once the import is done")
        recover(d)
        order = walk_list(r, d, "All")
        fixtures = [int(m.group(1)) for x in order if (m := FIXTURE.match(x))]
        r.check(len(fixtures) == LARGE, f"the list has all {LARGE} imported accounts ({len(fixtures)} seen)")
        r.check(fixtures == sorted(fixtures), "the imported accounts keep their order")
        letter_filter(r, d)
        d.swipe(540, 700, 540, 2100, 120)
        d.tap("Work", settle=1.5)
        order = walk_list(r, d, "Work")
        work = [int(m.group(1)) for x in order if (m := FIXTURE.match(x))]
        r.check(
            len(work) == LARGE // 3 and all(n % 3 == 1 for n in work),
            f'"Work" shows its {LARGE // 3} accounts ({len(work)})',
        )
        d.swipe(540, 700, 540, 2100, 120)
        d.tap("All", settle=1.0)
    finally:
        d.shell(f"rm -f {remote}")


def erase(r: Run):
    d = r.d
    open_settings(d)
    scroll_down(d)
    d.tap("reset", settle=1.0)
    d.tap("Erase Local Data", settle=1.5)
    s = d.wait("Verify your identity", timeout=3)
    if s.has("Verify your identity"):
        d.fill("Master Password", PASSWORD)
        d.key(66, settle=2.5)
    s = d.wait("Start fresh", timeout=8)
    r.check(s.has("Start fresh"), "erasing the vault goes back to the welcome page")


STEPS = [
    ("onboarding", onboarding),
    ("biometrics-off", settings_biometrics_off),
    ("scan", scan),
    ("manual-entry", manual_entry),
    ("paste-links", paste_links),
    ("reorder", reorder_accounts),
    ("details", details),
    ("categories", categories),
    ("search", search),
    ("delete-recover", delete_and_recover),
    ("settings-dialogs", settings_dialogs),
    ("language", language),
    ("export", export),
    ("lock", lock),
    ("back", back_navigation),
    ("large-import", large_import),
    ("erase", erase),
]


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument(
        "--serial", default=os.environ.get("ANDROID_SERIAL"), help="the device (adb devices); needed with more than one"
    )
    p.add_argument(
        "--apk", type=Path, default=APK, help=f"the demo package to install (default {APK.relative_to(ROOT)})"
    )
    p.add_argument("--no-install", action="store_true", help="use the demo package already on the device")
    p.add_argument(
        "--only",
        help="comma-separated steps to run after the ones that set up the vault and its accounts: "
        + ", ".join(n for n, _ in STEPS[1:]),
    )
    args = p.parse_args()

    if not shutil.which("android"):
        sys.exit("the android command-line tool is not on PATH (Android SDK cmdline-tools/latest/bin)")
    serial = args.serial
    if not serial:
        lines = subprocess.run([ADB, "devices"], capture_output=True, text=True).stdout.splitlines()[1:]
        devices = [line.split()[0] for line in lines if line.strip().endswith("device")]
        if len(devices) != 1:
            sys.exit(f"pass --serial: {len(devices)} devices online ({', '.join(devices)})")
        serial = devices[0]

    shots = ROOT / "build/android/ui-test" / time.strftime("%Y%m%d-%H%M%S")
    shots.mkdir(parents=True, exist_ok=True)
    d = Device(serial, shots)
    if not args.no_install:
        if not args.apk.exists():
            sys.exit(f"{args.apk} is missing: python3 scripts/android.py --demo")
        print(f"installing {args.apk.name}")
        d.adb("install", "-r", str(args.apk), check=True)
    d.shell(f"pm clear {PACKAGE}")
    # The camera page asks for the camera: granted here, as the system's prompt is no part of the app.
    d.shell(f"pm grant {PACKAGE} android.permission.CAMERA")
    # The screen stays on through the run; the device's own timeout comes back at the end.
    timeout = d.shell("settings get system screen_off_timeout")
    d.shell("settings put system screen_off_timeout 1800000")
    d.launch()

    steps = STEPS
    if args.only:
        wanted = set(args.only.split(","))
        # Later steps use the accounts the first ones add.
        setup = {"onboarding", "biometrics-off", "manual-entry", "paste-links"}
        steps = [s for s in STEPS if s[0] in wanted or s[0] in setup]
    run = Run(d)
    try:
        for name, fn in steps:
            run.step(name, fn)
    finally:
        d.shell(f"pm clear {PACKAGE}")
        if timeout.isdigit():
            d.shell(f"settings put system screen_off_timeout {timeout}")
    return run.report()


if __name__ == "__main__":
    sys.exit(main())
