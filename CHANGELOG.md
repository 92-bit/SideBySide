# What's new in SideBySide

Each release's notes are taken from its section below and shown in the app's update window.

## 0.1.1

A whole new look, and lots of fixes, big and small.

New look:
- A fresh design: the two files sit side by side as cards, with a pin at each difference
- 8 colour themes (Aurora, Paper, Signal, Midnight, Dusk, Forest, Glacier, High contrast), each in dark and light, or matching Windows. Pick one with the Theme button at the top
- The main buttons (next/previous difference, Find, Go to line, Ignore) sit together in the top bar
- A new start screen: pick or drop your two files, or paste text, and see your recent comparisons
- Clearer fonts, built into the app (Geist and Geist Mono)
- Small, calm animations that stay out of your way (turned off if Windows is set to reduce motion)

Comparing:
- Files saved as UTF-16 (common with some Windows programs) can now be compared
- Old Mac files, with lines ending in "\r" only, now show their lines properly
- A block of more than 20,000 added or removed lines no longer marks the rest of the file as changed
- "Ignore blank lines" counts and pairs the changed lines correctly
- If a file changes after comparing, a bar offers to compare again
- Extremely long lines (tens of thousands of characters) scroll smoothly
- Scrolling sideways reaches the end of lines with tabs, Chinese/Japanese text or emoji

Using the app:
- F7 / F8 pressed quickly move one difference per press
- Find works for all letters (like "Ä"), and works again after a cancelled comparison
- Go to line accepts numbers like 1,000,000
- Dropping one file puts it on the side you dropped it on
- Dialogs keep the keyboard inside them, and Escape always closes them
- Error messages now show on the paste screen

Installing and updating:
- After an update, the new version always opens, even on a slow computer
- Uninstalling asks you to close SideBySide first if it's still open
- Update information is now signed too, for extra safety

## 0.1.0

The first release of SideBySide by 92-bit.

- Compare two text files side by side, from tiny to very large (hundreds of MB)
- Changed words are highlighted inside each changed line
- Jump between differences with F7 / F8, or click the difference map
- Ignore spaces, blank lines, or upper/lower case
- Find (Ctrl+F) and Go to line (Ctrl+G)
- Compare pasted text without saving files
- Recent comparisons, light and dark mode, zoom
- Runs from a single file; click Install to add it to the Start Menu and the right-click menu
- Updates itself safely: every update is signed and checked before it is used
