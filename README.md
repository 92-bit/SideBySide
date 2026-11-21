<div align="center">

<img src="assets/icon.svg" alt="SideBySide logo" width="112" height="112">

# SideBySide

### by 92-bit

**See exactly what changed between two text files — instantly, even when they're huge.**

Fast · Lightweight · Free forever · Open source

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-3d4f80?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-7aa2f7?style=flat-square)](#-download)
[![Latest release](https://img.shields.io/github/v/release/92-bit/SideBySide?style=flat-square&color=3fb950&label=download)](https://github.com/92-bit/SideBySide/releases/latest)

</div>

---

## ✨ Why SideBySide?

Most comparison tools are either **heavy** or **struggle with large files**. SideBySide is built to do one job really well: put two files next to each other and show you every difference — clearly, quickly, and without slowing down your computer.

| | |
|---|---|
| ⚡ **Fast with huge files** | Compares files of hundreds of megabytes without freezing. |
| 🪶 **Light on your computer** | About 5 MB to download, and very little memory in use. |
| 🎒 **Portable** | One file. Run it from a folder or a USB stick — no install needed. |
| 📥 **Install if you like** | Prefer a normal app? Click **Install** inside SideBySide. No admin rights required. |
| 🔄 **Updates itself** | New versions arrive automatically, signed so you know they're genuine. |
| 🎨 **8 themes** | Aurora, Paper, Signal, Midnight, Dusk, Forest, Glacier and High contrast — each light and dark. |
| 🧰 **Much more than two files** | Folders, three files, Word / Excel / PDF text, pictures, and any file as bytes. |
| ✍️ **Edit & merge** | Copy a difference across with one click, edit a line, undo, then save. |
| 🌍 **5 languages** | English, Español, Français, Deutsch, 中文. |
| 🔒 **Private** | Your files never leave your computer. No accounts, no tracking. The only time it goes online is a once-a-day update check with GitHub (switch it off in **Help → About**), and when you open a web link yourself (<kbd>Ctrl</kbd>+<kbd>L</kbd>). |

---

## 📦 Download

### [⬇ Download SideBySide for Windows](https://github.com/92-bit/SideBySide/releases/latest/download/SideBySide.exe)

Linux: get the `.AppImage` or `.deb` from the [**latest release**](https://github.com/92-bit/SideBySide/releases/latest).
Mac: get the `.dmg` from the same page *(new — the Mac version hasn't been tested on a real Mac yet; the first time,
right-click the app and choose **Open**)*.

1. Download **`SideBySide.exe`** using the link above.
2. Double-click it. That's it — you're comparing.
3. *(Optional)* Click **Install** in the app to add it to your Start Menu and the right-click menu.
   To remove it later: **Settings → Apps → SideBySide → Uninstall**.

> 💡 **Seeing "Windows protected your PC"?** That's normal for new free apps.
> Click **More info → Run anyway**.

---

## 🗺️ Roadmap

### The essentials
- [x] Compare two files, from tiny to very large
- [x] Highlight the exact words that changed inside a line
- [x] Drag and drop files onto the window
- [x] Jump to the next / previous difference (<kbd>F7</kbd> / <kbd>F8</kbd>)
- [x] Ignore spaces, blank lines, or upper/lower case
- [x] Light and dark mode
- [x] Recent comparisons
- [x] Find (<kbd>Ctrl</kbd>+<kbd>F</kbd>) and *Go to line* (<kbd>Ctrl</kbd>+<kbd>G</kbd>)
- [x] Paste two pieces of text and compare them — no files needed
- [x] Progress bar with **Cancel** for big files
- [x] Zoom text in and out (<kbd>Ctrl</kbd> + mouse wheel)
- [x] Portable file with a built-in **Install** button (no admin rights needed)
- [x] Right-click **Compare with SideBySide** in File Explorer
- [x] Automatic updates, signed so they can't be faked

### More tools
- [x] A new look with 8 colour themes, light and dark
- [x] Colours for code (about 20 kinds of code files)
- [x] Live reload: follows a file as it changes (great for log files)
- [x] Bookmarks (<kbd>Ctrl</kbd>+<kbd>B</kbd>, <kbd>F2</kbd>), keyboard shortcut list (<kbd>F1</kbd>)
- [x] Font, text size, line spacing and word wrap
- [x] Reopen the last comparison at start, and tabs for several comparisons
- [x] Only the changed letters are marked inside a changed word
- [x] Ignore rules (for example, skip timestamps in logs)
- [x] Tidy up JSON and XML before comparing
- [x] Hide long runs of unchanged lines
- [x] Inline (one-column) view
- [x] Moved blocks of lines are shown as *moved*
- [x] Compare only part of a file (a range of lines)
- [x] Edit lines, copy differences across (merge), undo / redo and save
- [x] Save a report (HTML, prints to PDF) or copy a patch
- [x] Command line, and use as Git's compare tool (**Help → Use with Git**)
- [x] Compare with the clipboard (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd>) or a web link (<kbd>Ctrl</kbd>+<kbd>L</kbd>)
- [x] Compare folders, and three files (original + two versions)
- [x] Word, Excel, PowerPoint and PDF files (by their text), pictures, and any file as bytes (hex)
- [x] English, Español, Français, Deutsch, 中文

### Ideas welcome
Have an idea? Open an [issue](https://github.com/92-bit/SideBySide/issues).

---

## ⌨️ Command line

```bash
sidebyside old.txt new.txt                      # open them in the window
sidebyside old.txt new.txt --summary            # print how many lines differ
sidebyside old.txt new.txt --patch > changes.diff
sidebyside old.txt new.txt --report report.html --ignore-times
sidebyside folderA folderB                      # compare two folders
sidebyside original.txt a.txt b.txt             # three files
```

The exit code is **0** when the files are the same, **1** when they differ and **2** if something went wrong.
`sidebyside --help` lists every option. In PowerShell, add `| Write-Output` so it waits for the result.

---

## 🧠 How it handles huge files

Think of reading a very long book:

1. **One page at a time** — SideBySide reads files in small pieces, never all at once.
2. **A fingerprint for every line** — each line becomes a short code. Comparing codes is far faster than comparing whole lines.
3. **Only draw what you see** — the screen shows about 50 lines. The millions of others wait until you scroll to them.

**Measured:** two 500 MB files (5.1 million lines each) are compared in about **1.3 seconds**,
using at most **~85 MB** of memory while comparing. Scrolling anywhere in the file is instant.

---

## 🛠️ For developers

SideBySide is built with [Tauri 2](https://tauri.app) (Rust) and [Svelte 5](https://svelte.dev).

**You'll need:** [Node.js](https://nodejs.org) 20+, [Rust](https://rustup.rs), and on Windows the
[Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/).

```bash
git clone https://github.com/92-bit/SideBySide.git
cd SideBySide
npm install
npm run tauri dev        # run the app while developing
npm run tauri build      # build the finished app
```

---

## 🤝 Contributing

Ideas, bug reports and translations are very welcome!
Open an [issue](https://github.com/92-bit/SideBySide/issues) to say hello.

---

## 📜 License

SideBySide is free software under the [GNU General Public License v3.0](LICENSE).
You can use, share and change it — and any shared copies must stay free and open too.

<div align="center">
<br>
<sub>Made with care by <b>92-bit</b></sub>
</div>
