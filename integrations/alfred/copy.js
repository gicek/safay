#!/usr/bin/osascript -l JavaScript
// Alfred Run Script for Safay: receives an account id (never a secret) from the Script Filter and asks Safay's
// `copy code`. Safay asks for Touch ID or the master password, copies the code and shows its own notice; the code never
// comes back here. Prints text only when Safay could not copy, for Alfred's notification (empty: no notification).
ObjC.import("Foundation");
ObjC.import("AppKit");

const STRINGS = @@SAFAY_STRINGS@@;

function bridge(command, argument) {
  const task = $.NSTask.alloc.init;
  task.executableURL = $.NSURL.fileURLWithPath("/usr/bin/osascript");
  task.arguments = $(["safay.applescript", command, argument]);
  const out = $.NSPipe.pipe;
  const err = $.NSPipe.pipe;
  task.standardOutput = out;
  task.standardError = err;
  if (!task.launchAndReturnError($())) return "failed";
  const output = $.NSString.alloc.initWithDataEncoding(out.fileHandleForReading.readDataToEndOfFile, $.NSUTF8StringEncoding).js;
  const error = $.NSString.alloc.initWithDataEncoding(err.fileHandleForReading.readDataToEndOfFile, $.NSUTF8StringEncoding).js;
  task.waitUntilExit;
  if (task.terminationStatus === 0) return output.trim();
  return error.includes("-1743") ? "permission" : "failed";
}

function open(target) {
  const workspace = $.NSWorkspace.sharedWorkspace;
  if (target === "__safay_open_automation__") {
    workspace.openURL($.NSURL.URLWithString("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"));
    return;
  }
  for (const id of ["com.gicek.safay", "com.gicek.safay.debug"]) {
    const url = workspace.URLForApplicationWithBundleIdentifier(id);
    if (!url.isNil()) {
      workspace.openURL(url);
      return;
    }
  }
}

function run(argv) {
  const argument = argv[0] || "";
  if (argument.startsWith("__safay_open_")) {
    open(argument);
    return "";
  }
  const language = $.NSProcessInfo.processInfo.environment.objectForKey("safay_language").js;
  const t = STRINGS[language] || STRINGS.en;
  const messages = {
    failed: t.failed,
    busy: t.busy,
    not_found: t.notFound,
    disabled: t.disabledTitle,
    not_ready: t.notReadyTitle,
    not_running: t.notRunningTitle,
    permission: t.permissionTitle,
  };
  return messages[bridge("copy", argument)] || "";
}
