// Screenshots of every page (docs/screenshots, not tracked; the README copies a few into docs/img), taken on the macOS backend as the window server composites the
// windows (shadows, sheets and materials included). Public synthetic accounts only: the demo vault of
// previews/Demo*.tsx and a fresh in-memory vault.
//
//   python3 scripts/nsh.py test previews/screenshots.test.tsx --target macos --foreground
//
// `--foreground` lets the app become active: its window has the focus, so selections show the accent (an inactive
// window's are gray, as AppKit's).
//
// Headless runs (`nsh test` of the whole directory) cannot take pictures: each test then ends after opening its app,
// and test files see no environment variable that could skip them instead.
import { open as openApp, test } from "nsh/test";
import type { App, OpenOptions } from "nsh/test";

const dir = "docs/screenshots"; // relative to the directory nsh test runs in: the project root
const minute = 60_000;

function options(width: number, height: number): OpenOptions {
  return { width, height, platform: "macos", storage: {}, network: {}, clock: {} };
}

/** The app, or `null` where no platform draws it (headless). */
async function open(entry: string, size: OpenOptions): Promise<App | null> {
  const app = await openApp(entry, size);
  try {
    await app.screenshot();
  } catch {
    return null;
  }
  // The window takes the focus once the app is active (`--foreground`); without it, the pictures show it inactive.
  try {
    await app.waitUntil(() => app.window("main").state()?.focused === true, { timeout: 3000 });
  } catch {}
  return app;
}

async function shot(app: App, name: string) {
  await app.wait(350);
  await app.screenshot(`${dir}/${name}.png`);
}

test("main window: accounts, layouts and categories", async () => {
  const app = await open("./Demo.tsx", options(980, 680));
  if (!app) return;
  await app.waitFor("GitHub");
  await shot(app, "accounts-light");
  await app.setColorScheme("dark");
  await shot(app, "accounts-dark");
  await app.press("role:button[List]");
  await shot(app, "list-dark");
  await app.setColorScheme("light");
  await shot(app, "list-light");
  await app.press("role:button[Cards]");
  await app.setViewport(1280, 800);
  await shot(app, "accounts-wide");
  await app.setViewport(880, 620);
  await shot(app, "accounts-compact");
  await app.setViewport(980, 680);
  await app.press("role:button[Work]");
  await shot(app, "category");
  await app.press("role:button[All]");
  await app.press("role:button[Copy code for GitHub]");
  await app.waitFor("Copied");
  await shot(app, "copied");
}, 2 * minute);

test("main window: sheets", async () => {
  const app = await open("./Demo.tsx", options(980, 680));
  if (!app) return;
  await app.waitFor("GitHub");
  await app.press("role:button[Add]");
  await app.waitFor("Add Account");
  await shot(app, "add-scan");
  await app.press("role:button[Enter Key]");
  await shot(app, "add-key");
  await app.press("role:button[Paste Links]");
  await shot(app, "add-text");
  await app.keyboard.press("Escape");
  await app.press("role:button[Details] >> nth=0");
  await app.waitFor("Account Details");
  await shot(app, "detail");
  await app.press("role:button[Show Secret]");
  await app.waitFor("Verify It’s You");
  await shot(app, "verify");
  await app.keyboard.press("Escape");
  await app.wait(200);
  await app.keyboard.press("Escape");
  await app.press("role:button[Import / Export]");
  await app.waitFor("Batch Import");
  await shot(app, "import");
  await app.press("role:button[Encrypted Export]");
  await shot(app, "export");
  await app.press("role:button[Cancel]");
  await app.press("role:button[Backup & Sync]");
  await app.waitFor("Backup is off");
  await shot(app, "backup-off");
  await app.press("role:button[Set Up]");
  await app.waitFor("Other Folder");
  await shot(app, "backup-locations");
  // A self-signed server (the debug fixture answers so for this name): its fingerprint, for the user to compare.
  await app.press("role:button[WebDAV]");
  await app.type("id:cloud-address", "https://self-signed.example.invalid/dav");
  await app.type("id:cloud-username", "fixture");
  await app.type("id:cloud-password", "synthetic");
  await app.press("role:button[Connect]");
  await app.waitFor("The server’s certificate can’t be verified");
  await shot(app, "webdav-certificate");
}, 2 * minute);

test("lock screen", async () => {
  const app = await open("./DemoLocked.tsx", options(980, 680));
  if (!app) return;
  await app.waitFor("Safay is locked");
  await shot(app, "locked-light");
  await app.setColorScheme("dark");
  await shot(app, "locked-dark");
}, minute);

test("first launch", async () => {
  // Reduced motion holds the sidebar's vault graphic on its plain code instead of a frame of its scramble.
  const app = await open("./Fresh.tsx", { ...options(980, 680), reducedMotion: true });
  if (!app) return;
  await app.waitFor("Protect your 2FA with Safay");
  await shot(app, "welcome");
  await app.press("role:button[Restore a backup]");
  await app.waitFor("Other Folder");
  await shot(app, "restore-locations");
  await app.press("role:button[Back]");
  await app.press("role:button[Start fresh]");
  await app.type("id:new-password", "P8!nQ2@vT7#sL4$m");
  await app.type("id:confirm-password", "P8!nQ2@vT7#sL4$m");
  await shot(app, "create");
  await app.press("role:button[Create Vault]");
  await app.waitFor("No 2FA accounts yet");
  await shot(app, "empty");
  // A public RFC 6238 sample: its import confirms itself in the window's toast.
  await app.press("role:button[Add]");
  await app.press("role:button[Paste Links]");
  await app.type("id:import-text", "otpauth://totp/Example:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Fixture&issuer=Example");
  await app.waitFor("Parsed");
  await app.press("role:button[Save Account]");
  await app.waitFor("Imported 1 account");
  await shot(app, "toast");
}, minute);

test("menu bar panel", async () => {
  const app = await open("./DemoPanel.tsx", options(310, 452));
  if (!app) return;
  await app.waitFor("GitHub");
  await shot(app, "panel-light");
  await app.press("role:button[Copy code for GitHub]");
  await app.waitFor("Copied");
  await shot(app, "panel-copied");
  await app.type("id:panel-search", "nothing-matches");
  await app.waitFor("No matching accounts");
  await shot(app, "panel-no-results");
  await app.clear("id:panel-search");
  await app.setColorScheme("dark");
  await app.waitFor("GitHub");
  // After the copy feedback has gone.
  await app.wait(1500);
  await shot(app, "panel-dark");
}, minute);

test("Settings window", async () => {
  const app = await open("./DemoSettings.tsx", options(760, 520));
  if (!app) return;
  await app.waitFor("Appearance");
  await shot(app, "settings-general");
  for (const [page, name] of [["Security", "security"], ["Shortcuts", "shortcuts"], ["Advanced", "advanced"], ["About", "about"]]) {
    await app.press(`role:button[${page}]`);
    await shot(app, `settings-${name}`);
  }
  await app.press("role:button[General]");
  await app.setColorScheme("dark");
  await shot(app, "settings-dark");
  await app.setColorScheme("light");
  await app.press("role:combobox[Language]");
  await app.press("简体中文"); // i18n-guard: allow
  await app.waitFor("外观"); // i18n-guard: allow
  await shot(app, "settings-zh-Hans");
}, 2 * minute);
