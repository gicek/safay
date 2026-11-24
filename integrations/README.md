# Launcher integrations

Alfred, Raycast and Tinycast search Safay's accounts and ask Safay to copy a code. They call two AppleScript commands
(`resources/Safay.sdef`); Safay asks for Touch ID or the master password before every copy and never returns the code.
Design: [Architecture](../docs/ARCHITECTURE.md#launcher-integration); rules: [Security](../SECURITY.md#launcher-integration-macos).

| Folder | Contents |
|---|---|
| `shared/safay.applescript` | The bridge all launchers run: `find` / `copy`, arguments passed as argv |
| `raycast/` | One extension for Raycast and Tinycast (TypeScript, MIT) |
| `alfred/` | The Alfred workflow's scripts; `scripts/integrations.py --alfred` builds `Safay.alfredworkflow`, which every package ships |

`python3 scripts/integrations.py` generates the extension's text and bridge (`raycast/src/generated.ts`) from
`locales/en.json` and `shared/`; `make lint` fails when it is stale. The Raycast Store accepts US English only, so the
extension is English; the Alfred workflow follows Safay's language.

## Setup

Every launcher needs Safay → Settings → Advanced → **Launcher Access**, and permission to control
Safay, which macOS asks for on first use (System Settings → Privacy & Security → Automation).

- **Alfred** (Powerpack): Safay → Settings → Advanced → **Install to Alfred**. Keyword `sf`.
- **Raycast**: Safay → Settings → Advanced → **Install to Raycast**, or install
  [Safay from the Raycast Store](https://raycast.com/kakegu/safay). Command "Search Accounts".
- **Tinycast**: Safay → Settings → Advanced → **Install to Tinycast**, which installs the same extension from the
  Raycast Store.

Safay shows the Raycast and Tinycast buttons only when that launcher is installed. To try local changes, run
`make raycast`, then `npm run dev` in `raycast/` to load the extension into Raycast, or add `raycast/dist` to Tinycast
with Settings → Extensions → Install → **Add from folder**.

Raycast and Tinycast leave aliases to the user. The command's keywords include `sf`; to make `sf` always pick it first,
set the alias in the launcher's extension settings.

## Updating the Raycast Store extension

The extension is published as `kakegu/safay`. Change it only in `raycast/`, then:

1. Add a section to `raycast/CHANGELOG.md` dated `{PR_MERGE_DATE}`; the store fills in the date when it merges.
2. Run `make raycast`, then `npm run publish` in `raycast/`. It opens a pull request to `raycast/extensions`.
3. After the merge, copy the store's changes back into `raycast/` with `npx ray pull-contributions`. The store dates
   the changelog and compresses the images, and the next publish compares against its copy.

Screenshots are 2000 × 1250 PNGs in `raycast/metadata/` (3–6), taken with Raycast's **Capture Window** →
**Save to Metadata**, using synthetic accounts (`make preview`) and one appearance.

Settings installs the extension with `raycast://extensions/kakegu/safay?source=webstore` and
`tinycast://extensions/kakegu/safay?source=webstore`.
