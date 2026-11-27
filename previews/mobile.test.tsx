// Screenshots of the phone and tablet interface over the demo vault, page by page, with the platform's own input
// (build/screenshots/mobile, not tracked). Public synthetic accounts only.
//
//   python3 scripts/nsh.py test previews/mobile.test.tsx --target ios [--device "iPad Pro 11-inch (M5)"]
//   python3 scripts/nsh.py test previews/mobile.test.tsx --target android
//
// Headless runs take no pictures: each test then ends after opening its app.
import { expect, open as openApp, test } from "nsh/test";
import type { App, OpenOptions } from "nsh/test";

const dir = "build/screenshots/mobile";
const minute = 60_000;
const options: OpenOptions = { storage: {}, network: {}, clock: {} };

/** The app, or `null` where no platform draws it (headless). */
async function open(entry: string): Promise<App | null> {
  const app = await openApp(entry, options);
  try {
    await app.screenshot();
  } catch {
    return null;
  }
  return app;
}

/** Chooses `label` in the menu that is open: the system's (iOS) or the app's own (`id:` of its row). */
async function pick(app: App, label: string) {
  try {
    await app.chooseMenuItem(label);
  } catch {
    await app.press(`id:${label}`);
  }
}

/** Closes the menu that is open: the system's, or the app's own by its button again. */
async function closeMenu(app: App, button: string) {
  try {
    await app.closeMenu();
  } catch {
    await app.press(button);
  }
}

/** Closes the sheet that is open: a swipe down on it (iOS's system sheet), or back elsewhere. */
async function closeSheet(app: App, on: string) {
  try {
    await app.drag(on, { y: 900 });
    await app.wait(800);
  } catch {}
  try {
    await app.back();
  } catch {}
}

async function shot(app: App, name: string) {
  await app.wait(600);
  await app.screenshot(`${dir}/${name}.png`);
}

test("the list, its menus and the account's sheets", async () => {
  const app = await open("./DemoMobile.tsx");
  if (!app) return;
  await app.waitFor("GitHub");
  await shot(app, "home");
  await app.press("id:add");
  await shot(app, "add-menu");
  await closeMenu(app, "id:add");
  await app.press("id:more");
  await shot(app, "more-menu");
  await closeMenu(app, "id:more");
  await app.press("label:Work");
  await shot(app, "category");
  await app.press("label:All");
  await app.press("label:Copy code for GitHub");
  await shot(app, "copied");
  await app.longPress("label:Copy code for GitHub");
  await shot(app, "context-menu");
  await pick(app, "Details");
  await app.waitFor("Algorithm");
  await shot(app, "detail");
  // At half the screen the sheet's content scrolls.
  await app.scroll("text:Algorithm", 500);
  await shot(app, "detail-scrolled");
  await app.press("id:qr");
  await app.waitFor("Verify your identity");
  await shot(app, "verify");
  await app.back();
  await app.press("id:edit");
  await shot(app, "detail-edit");
  await app.press("id:done");
  await closeSheet(app, "id:code");
  // A sheet opens again with its content, once the last one has gone.
  await app.longPress("label:Copy code for Google");
  await pick(app, "Details");
  await app.waitFor("Algorithm");
  await shot(app, "detail-again");
  await closeSheet(app, "id:code");
  await app.longPress("label:Copy code for Google");
  await pick(app, "Add to");
  await shot(app, "add-to");
  await closeSheet(app, "id:done");
  await app.press("id:search-button");
  await app.type("id:search", "git");
  await shot(app, "search");
  await app.press("id:search-button");
  await app.setColorScheme("dark");
  await shot(app, "home-dark");
  await app.setColorScheme("light");
  // iOS reorders by drag handles, from "More".
  await app.press("id:more");
  let reorders = true;
  try {
    await app.chooseMenuItem("Reorder");
  } catch {
    reorders = false;
    await closeMenu(app, "id:more");
  }
  if (reorders) {
    await shot(app, "reorder");
    // The first row's handle, dragged two rows down.
    await app.drag("id:grip >> nth=0", { y: 150 });
    await shot(app, "reordered");
    await app.press("id:done");
  }
}, 5 * minute);

test("pages over the list, and going back", async () => {
  const app = await open("./DemoMobile.tsx");
  if (!app) return;
  await app.waitFor("GitHub");
  await app.press("id:more");
  await pick(app, "Settings");
  await app.waitFor("Auto-Lock");
  await shot(app, "settings");
  await app.press("label:Auto-Lock");
  await shot(app, "auto-lock");
  await closeMenu(app, "label:Auto-Lock");
  await app.press("id:change-password");
  await app.type("id:current-password", "short");
  await app.type("id:new-password", "another");
  await shot(app, "change-password");
  await app.back();
  await app.scroll("text:Auto-Lock", 120);
  await shot(app, "settings-scrolled");
  await app.scroll("text:Auto-Lock", 900);
  await shot(app, "settings-end");
  await app.setColorScheme("dark");
  await shot(app, "settings-dark");
  await app.setColorScheme("light");
  // The system's back leads to the list.
  await app.back();
  await app.waitFor("GitHub");
  await shot(app, "after-swipe");
  await app.press("id:manage");
  await app.waitFor("Favorites");
  await shot(app, "categories");
  // iOS: the system's alert, which the test answers with its cancel button.
  await app.press("id:new-category");
  await app.wait(800);
  if (app.dialogs().length > 0) expect(app.dialogs().at(-1)).toMatchObject({ title: "New Category" });
  await app.back();
  await app.press("id:add");
  await pick(app, "Enter Setup Key");
  await shot(app, "manual");
  await app.type("id:issuer", "Example");
  await shot(app, "manual-typing");
  await app.back();
  await app.press("id:add");
  await pick(app, "Scan QR Code");
  await shot(app, "scan");
}, 5 * minute);

test("locked and first launch", async () => {
  const locked = await open("./DemoMobileLocked.tsx");
  if (!locked) return;
  await locked.waitFor("Safay is locked");
  await shot(locked, "locked");
  await locked.type("id:password", "wrong password");
  await shot(locked, "locked-typing");
  const fresh = await open("./FreshMobile.tsx");
  if (!fresh) return;
  await fresh.waitFor("Start fresh");
  await shot(fresh, "welcome");
  await fresh.press("id:Start fresh");
  await shot(fresh, "create");
  await fresh.back();
  await fresh.press("id:Restore a backup");
  await shot(fresh, "restore");
}, 5 * minute);
