# Interface design

Safay is a compact, quiet desktop authenticator: the main task is finding an account and copying its current code.
The visual language is an indigo accent taken from the icon's shield, adaptive compact cards, coloured monograms, codes in Atkinson Hyperlegible Mono,
ring countdowns and grouped settings.

## One layout, native on each platform

Layout, information architecture and behaviour are the same everywhere. Geometry and colour follow each platform; they
are defined in `src/chrome.rs` and the three theme files, and pages read them through `chrome::look()`, which affects
appearance only — never capabilities.

| | macOS (`theme.json`) | Windows (`theme-windows.json`) | Linux (`theme-linux.json`) |
|---|---|---|---|
| Title bar | Hidden; a 52 pt toolbar row with the traffic lights over the sidebar | 48 pt, system caption buttons on the right | 46 pt header bar, window buttons where GTK reports them |
| Sidebar | 210 pt; floating on macOS 26, flush on 15 and earlier and on 27 and later; accent-filled selection | 240 pt, Mica tone, 3 pt accent bar on the selection | 220 pt, flush |
| Controls | 30/36 pt, radius 6/8 | 32 pt, radius 4 | 34/38 pt, radius 6 |
| Settings | Separate 760 × 520 window (⌘,) | Dialog over the main window | Dialog over the main window |

## Colour and type

- Colours come only from theme tokens; source code has no colour constants (box shadows excepted: NSH takes a fixed
  colour there, so they use low-opacity black).
- Body and controls use the system font at 13 pt. Codes use Atkinson Hyperlegible Mono (bundled, OFL): 21 pt on cards,
  19 pt in the list, 26 pt in details, 14 pt in the menu bar panel. Revealed passwords and keys use the same monospaced
  face, which tells `1`, `l` and `I` apart.
- Every platform uses the same accent. White text on accent buttons meets 4.5:1 (`#3d5fa8` light, `#4f6fb8` dark); in
  dark mode, accent text and glyphs use a lighter `#8aa5e0`.

## Structure

- **Main window**: sidebar and toolbar (title, lock, list/cards, search, add); a card grid (minimum 210 pt, 12 pt
  gaps) or a 58 pt list. Minimum size 880 × 620. The sidebar is one list without section titles: All, then the
  categories, with "New Category" (a folder with a plus) left of the sidebar toggle in its title bar; Import/Export,
  Backup & Sync and Settings sit at its foot. An empty vault shows no buttons: hand-drawn arrows point from short notes
  to the toolbar's add button and to "Import / Export".
- **Account actions** use native context menus: copy, add to or remove from categories, details, QR code, move up/down,
  delete. Cards and rows can be dragged to reorder; categories too.
- **Sheets**: Add Account 620 pt (scan, key, links), Details 540, QR code 420, Import/Export 660, Change Password 460,
  Backup & Sync 640. A 56 pt header, 22 × 18 pt content padding, a footer with the actions and the only error line.
- **Lock screen**: the app icon, a 220 pt capsule password field, Return to unlock; with Touch ID, a fingerprint button
  and an automatic prompt on first launch.
- **First launch**: an introduction beside two choices, Start Fresh and Restore a Backup; restoring lists every backup
  location and Safay export files.
- **Menu bar panel**: 310 pt wide; search, 44 pt rows, arrow keys and Return to copy, select-on-screen in the footer,
  category scope; it closes on Escape or when focus leaves.
- **Desktop card**: the glass card of the desktop notice and of the selection's instructions: 56 pt tall, the 36 pt
  app icon 10 pt in, its 8 pt corners concentric with the card's 18 pt ones. It sits 44 pt under the top of the visible
  area of the display under the pointer.
- **Select on screen**: every display dims under an overlay that never activates Safay. 2FA QR codes found on screen
  get an accent outline and an "Add <issuer>" capsule ("Already in Safay" when the vault has them); a click on either
  adds the code, and a drag frames any area with viewfinder corners. A badge by the pointer shows a square viewfinder
  and Safay's name, since macOS keeps the cursor of an app in the background. The desktop card says what Safay found,
  with "esc Cancel" as a button; anything under the pointer that takes a click lights up.
- **Desktop notice**: the desktop card with the result's symbol on the app icon, for 2.6 s, never activating Safay or
  taking input (results of on-screen QR selection and launcher copies).
- **Verification**: with Touch ID set up, the system prompt comes first; Safay's own 360 pt dialog (master password,
  plus a button to retry Touch ID) appears only without Touch ID or when it did not verify.

## Behaviour

- **Modals** share `components::modal_surface` and `scrim`: in dark mode the surface is a step lighter than the window
  with a hairline rim, over a 40 % black scrim. The scrim is a window-drag region, so the window can be moved while a
  modal is open. A sheet scrolls as a whole only when the window is too short for it (`components::overflow_scroll`,
  without bounce); lists inside sheets have a fixed height and scroll on their own.
- **Validation**: a new password's length and strength are shown after leaving the field; the confirmation is checked
  once the new password is acceptable, and a mismatch appears at the first wrong character. Error lines keep their
  height so forms do not jump. Strength is evaluated once per change, never per keystroke.
- **Fair passwords** ask before use; the default button is "Choose Stronger Password". The same rules apply when
  creating a vault, changing the password, restoring with a new password and exporting.
- **Feedback**: copying shows "Copied" in place; imports and additions show a toast; launcher copies show a desktop
  notice naming the account, never the code.
- **Focus**: `components::focus_ring` draws a 1 pt ring inside the element; focus never adds a border, which would take
  layout space.
- **Motion**: transitions are 160–240 ms ease-out (the welcome cards use a slight overshoot); every animation is
  skipped with Reduce Motion.
- **Password and key fields** accept only Roman input sources on macOS.
- When the window is inactive, sidebar selection and segmented controls turn grey (`chrome::window_active`).

## Windows and state

All windows — main window, settings window, menu bar panel, desktop notice — are rendered by one NSH engine. Each
window's content has its own `Store` and shares the one controller ([Architecture](ARCHITECTURE.md#session-model)).
Requests between windows (open a sheet in the main window, run an on-screen capture) carry only a kind, never data.

## Checking a change

Check widths 880, 980 and 1280; light and dark; all three languages; empty, no-result and error states; every sheet,
the settings pages and the menu bar panel; keyboard navigation, VoiceOver, larger text and Reduce Motion. A successful
build or a good-looking screenshot is not enough. Report a platform as checked only after running it there.
