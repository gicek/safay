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

Every launcher needs Safay → Settings → Advanced → **Enable Third-Party Plugin Interface**, and permission to control
Safay, which macOS asks for on first use (System Settings → Privacy & Security → Automation).

- **Alfred** (Powerpack): Safay → Settings → Advanced → **Install to Alfred**. Keyword `sf`.
- **Raycast**: the "Search Accounts" command. Until the extension is in the Raycast Store, run `make raycast`, then
  `npm run dev` in `raycast/` to load it.
- **Tinycast**: Settings → Extensions → Install → **Import from Raycast**, or **Add from folder** with `raycast/dist`
  (built by `make raycast`).

Raycast and Tinycast leave aliases to the user. The command's keywords include `sf`; to make `sf` always pick it first,
set the alias in the launcher's extension settings.

## Publishing to the Raycast Store

`npm run publish` in `raycast/` opens the pull request to `raycast/extensions`. Before that, add 3–6 screenshots
(2000 × 1250 PNG) to `raycast/metadata/` with Raycast's **Capture Window** → **Save to Metadata**, using synthetic
accounts (`make preview`) and one appearance.

Once published, Safay's settings can install it directly with `raycast://extensions/kakegu/safay?source=webstore` and
`tinycast://extensions/kakegu/safay?source=webstore`.
