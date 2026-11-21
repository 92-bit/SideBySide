# What's new in SideBySide

Each release's notes are taken from its section below and shown in the app's update window.

## 1.0.0

The first release of SideBySide by 92-bit: a fast, light app that compares files side by side.

Comparing:
- Compare two text files side by side, from tiny to very large (hundreds of MB in about a second)
- Changed words are highlighted inside each changed line, down to the letters that changed
- Blocks of lines that only moved are shown as moved, not as removed and added
- Ignore spaces, blank lines, upper/lower case, or your own rules (like timestamps in logs)
- Tidy JSON and XML before comparing
- Compare only part of a file (a range of lines)
- Compare pasted text, what you copied (Ctrl+Shift+V) or a web page (Ctrl+L)
- Compare two whole folders, or three files (an original and two changed versions)
- Word, Excel, PowerPoint and PDF files are compared by their text
- Pictures: a slider, side by side, or only the changed pixels
- Files that aren't text are shown as bytes (hex view)
- Files saved as UTF-16 or with old Mac line endings work too
- Live reload follows a file while it changes (great for log files)

Reading and moving around:
- Jump between differences with F7 / F8, or click the difference map
- Find (Ctrl+F), Go to line (Ctrl+G) and bookmarks (Ctrl+B, then F2 / Shift+F2)
- Hide long runs of unchanged lines, or see both files in one column (inline view)
- Colours for about 20 kinds of code files
- Pick the font, text size, line spacing and whether long lines wrap
- Tabs for several comparisons, and reopen the last one when the app starts
- Press F1 to see every keyboard shortcut

Editing and sharing:
- Copy a difference across with one click, edit lines, undo / redo, then save
- Save a report (HTML, can be printed to PDF) or copy the changes as a patch
- Command line: `sidebyside old.txt new.txt --summary`, and use SideBySide as Git's compare tool (Help → Use with Git)

Look and languages:
- 8 colour themes (Aurora, Paper, Signal, Midnight, Dusk, Forest, Glacier, High contrast), each dark and light, or matching Windows
- English, Español, Français, Deutsch, 中文

Installing and updating:
- Runs from a single file; click Install to add it to the Start Menu and the right-click menu (no admin needed)
- Uninstall from Settings → Apps; nothing is left behind
- Updates itself safely: every update is signed and checked before it is used
- Also for Linux (.AppImage, .deb) and, new and not yet tested on a real Mac, macOS (.dmg)
