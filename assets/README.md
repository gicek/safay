# Assets

| File | Use |
|---|---|
| `icon.png` | App icon, exported from the design master; re-export rather than edit the PNG |
| `android/` | Android adaptive launcher icon layers, derived from `icon.png` by `scripts/android_icon.py`; run it again after the icon changes |
| `menubar-template.svg` | Menu bar icon (template image, tinted by the system, shown at 18 × 18 pt) |
| `fonts/AtkinsonHyperlegibleMono-*` | Verification-code font, Braille Institute / Applied Design Works, SIL OFL 1.1 (license alongside) |
| `brands/` | Backup-location marks; see [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) |

Fonts and icons are registered locally through NSH's asset system; nothing is downloaded at run time.
