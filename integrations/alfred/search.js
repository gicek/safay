#!/usr/bin/osascript -l JavaScript
// Alfred Script Filter for Safay (keyword `sf`). Asks Safay's `find accounts` through the shared bridge script and
// lists what it returns: service names and masked account names, never a code or a secret. The same states and text
// as the Raycast / Tinycast extension (docs/ARCHITECTURE.md, Launcher integration).
ObjC.import("Foundation");

const STRINGS = @@SAFAY_STRINGS@@;
const OPEN_SAFAY = "__safay_open_app__";
const OPEN_AUTOMATION = "__safay_open_automation__";

// The bridge with its arguments as argv (NSTask, no shell): the query is never part of any script's text.
function bridge(command, argument) {
  const task = $.NSTask.alloc.init;
  task.executableURL = $.NSURL.fileURLWithPath("/usr/bin/osascript");
  task.arguments = $(["safay.applescript", command, argument]);
  const out = $.NSPipe.pipe;
  const err = $.NSPipe.pipe;
  task.standardOutput = out;
  task.standardError = err;
  if (!task.launchAndReturnError($())) return { error: "" };
  const output = $.NSString.alloc.initWithDataEncoding(out.fileHandleForReading.readDataToEndOfFile, $.NSUTF8StringEncoding).js;
  const error = $.NSString.alloc.initWithDataEncoding(err.fileHandleForReading.readDataToEndOfFile, $.NSUTF8StringEncoding).js;
  task.waitUntilExit;
  return task.terminationStatus === 0 ? { output: output.trim() } : { error };
}

function systemLanguage() {
  const preferred = $.NSLocale.preferredLanguages.firstObject.js.toLowerCase();
  if (preferred.startsWith("zh")) return /hant|-tw|-hk|-mo/.test(preferred) ? "zh-Hant" : "zh-Hans";
  return "en";
}

function notice(t, language, title, subtitle, arg) {
  return { title, subtitle: subtitle || "", arg, valid: true, variables: { safay_language: language } };
}

function run(argv) {
  const query = argv[0] || "";
  let language = systemLanguage();
  const answer = bridge("find", query);
  let items;
  if (answer.error !== undefined) {
    const t = STRINGS[language];
    // -1743: this app is not allowed to control Safay (macOS Automation).
    items = answer.error.includes("-1743")
      ? [notice(t, language, t.permissionTitle, t.permissionSubtitle, OPEN_AUTOMATION)]
      : [notice(t, language, t.notRunningTitle, t.openSafay, OPEN_SAFAY)];
  } else if (answer.output === "not_running") {
    const t = STRINGS[language];
    items = [notice(t, language, t.notRunningTitle, t.openSafay, OPEN_SAFAY)];
  } else {
    let found;
    try {
      found = JSON.parse(answer.output);
    } catch (e) {
      found = { enabled: false, ready: false, accounts: [] };
    }
    if (found.language && STRINGS[found.language]) language = found.language;
    const t = STRINGS[language];
    if (!found.enabled) items = [notice(t, language, t.disabledTitle, t.disabledSubtitle, OPEN_SAFAY)];
    else if (!found.ready) items = [notice(t, language, t.notReadyTitle, t.notReadySubtitle, OPEN_SAFAY)];
    else if (found.accounts.length === 0) items = [{ title: t.noMatches, valid: false }];
    else
      items = found.accounts.map((a) => ({
        uid: a.id,
        title: a.issuer,
        subtitle: a.username,
        arg: a.id,
        icon: { path: "icon.png" },
        variables: { safay_language: language },
      }));
  }
  return JSON.stringify({ items });
}
