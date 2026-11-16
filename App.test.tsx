import { expect, open, test } from "nsh/test";
import type { OpenOptions } from "nsh/test";

const password = "P8!nQ2@vT7#sL4$m"; // Public test credential, never a production fallback.
const options = { width: 980, height: 680, platform: "macos" as const, storage: {}, network: {}, clock: {} } satisfies OpenOptions;

test("create, paste a public sample, copy, and lock", async () => {
  const app = await open("./previews/Fresh.tsx", options);
  await app.waitFor("Start fresh");
  await app.press("role:button[Start fresh]");
  await app.type("id:new-password", password);
  await app.type("id:confirm-password", password);
  await app.press("role:button[Create Vault]");
  await app.waitFor("No 2FA accounts yet");
  await app.press("role:button[Add]");
  await app.press("role:button[Paste Links]");
  await app.type("id:import-text", "otpauth://totp/RFC%20sample:fixture?secret=JBSWY3DPEHPK3PXP&issuer=RFC%20sample");
  await app.press("role:button[Save Account]");
  await app.waitFor("RFC sample");
  await app.press("role:button[Copy code for RFC sample]");
  await app.waitFor("Copied");
  await app.type("id:search", "no matching account");
  await app.waitFor("No matching accounts");
  await app.press("role:button[Lock]");
  await app.waitFor("Safay is locked");
  expect(app.texts()).not.toContain("RFC sample");
});

test("normal CLI runner cannot open production storage", async () => {
  const app = await open(".", options);
  expect(app.texts()).toContain("NSH UI harness. Launch the local vault with cargo run.");
});

test("cards and rows reorder by dragging", async () => {
  const app = await open("./previews/Fresh.tsx", options);
  const order = () => app.texts().filter((t) => t === "Alpha" || t === "Bravo" || t === "Charlie");
  await app.waitFor("Start fresh");
  await app.press("role:button[Start fresh]");
  await app.type("id:new-password", password);
  await app.type("id:confirm-password", password);
  await app.press("role:button[Create Vault]");
  await app.waitFor("No 2FA accounts yet");
  await app.press("role:button[Add]");
  await app.press("role:button[Paste Links]");
  await app.type(
    "id:import-text",
    [
      "otpauth://totp/Alpha:a?secret=JBSWY3DPEHPK3PXP&issuer=Alpha",
      "otpauth://totp/Bravo:b?secret=JBSWY3DPEHPK3PXQ&issuer=Bravo",
      "otpauth://totp/Charlie:c?secret=JBSWY3DPEHPK3PXR&issuer=Charlie",
    ].join("\n"),
  );
  await app.press("role:button[Add 3 Accounts]");
  await app.waitFor("Charlie");
  expect(order()).toEqual(["Alpha", "Bravo", "Charlie"]);
  await app.drag("role:button[Copy code for Alpha]", { x: 250, y: 0 });
  expect(order()).toEqual(["Bravo", "Alpha", "Charlie"]);
  expect(app.texts()).not.toContain("Copied");
  await app.press("role:button[List]");
  await app.drag("role:button[Copy code for Bravo]", { x: 0, y: 120 });
  expect(order()).toEqual(["Alpha", "Charlie", "Bravo"]);
});

test("categories reorder by dragging", async () => {
  const app = await open("./previews/Fresh.tsx", options);
  const order = () => app.texts().filter((t) => t === "Work" || t === "Home" || t === "Misc");
  await app.waitFor("Start fresh");
  await app.press("role:button[Start fresh]");
  await app.type("id:new-password", password);
  await app.type("id:confirm-password", password);
  await app.press("role:button[Create Vault]");
  await app.waitFor("No 2FA accounts yet");
  for (const name of ["Work", "Home", "Misc"]) {
    await app.press("role:button[New Category]");
    await app.type("id:category-name", name);
    await app.submit("id:category-name");
    await app.waitFor(`role:button[${name}]`);
  }
  await app.press("role:button[All]");
  await app.waitFor("No 2FA accounts yet");
  expect(order()).toEqual(["Work", "Home", "Misc"]);
  await app.drag("role:button[Work]", { x: 0, y: 64 });
  expect(order()).toEqual(["Home", "Misc", "Work"]);
  await app.drag("role:button[Misc]", { x: 0, y: -120 });
  expect(order()).toEqual(["Home", "Misc", "Work"]);
});
