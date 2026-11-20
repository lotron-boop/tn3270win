# tn3270win

A TN3270 terminal emulator for Windows, written in Rust, with a native Win32
interface: a real menu bar, standard dialogs and keyboard navigation. The
first priority is keyboard mapping.

## Status: milestone 3, sessions, selection and screen sizes

- **Saved sessions**: in Communication > Connect..., fill in the fields and
  click **Save As...** to store them under a name; pick a name from the
  Session box next time. Each session has its own screen size and, if you
  like, its own keyboard preset. Stored in `%APPDATA%\tn3270win\sessions`.
- **Screen sizes**: 24x80 (Model 2), 32x80 (Model 3), 43x80 (Model 4),
  27x132 (Model 5). As on a real 3278, the host switches to the large size
  with Erase/Write Alternate (ISPF does this).
- **Select text**: Edit > Select Text, drag a box with the mouse, then
  Edit > Copy (or your Copy key). Without Select Text, a click still places
  the cursor. Copy with nothing selected copies the whole screen.
- **View menu**: Fit Font to Window (on by default: the screen scales with
  the window), Larger Font and Smaller Font for a fixed size.

## Milestone 2: connecting to a host

- **Communication > Connect...**: host name, port (23 by default), optional
  LU name, and code page (037 US/Canada by default; also 1140, 500, 273).
  The last settings are remembered.
- TN3270E with automatic fallback to classic TN3270, unencrypted.
- 24x80 (model 2) screen: fields, protected/numeric/hidden fields, base and
  extended colours, reverse video and underline, the host's Query.
- Editing: typing with autoskip, insert mode, Tab/Back Tab, Home, New Line,
  End of Field, Delete, Backspace, Erase EOF, Erase Input, Dup, Field Mark,
  Reset, PF1-PF24, PA1-PA3, Clear, Attn. Click to place the cursor.
- Status line: connection, keyboard lock (X SYSTEM, X Protected...), insert
  mode, cursor row/column.
- Edit > Copy Screen and Paste (paste types into the fields).
- The screen font resizes with the window.

Not yet: encrypted connections (TLS), SysReq, printing, file transfer,
macros.

## Milestone 1: keyboard mapping

- **Main window**: a 24x80 screen (not connected to a host yet) in
  *keyboard test mode*. Press any key and the status line shows the key and
  the 3270 function it is mapped to.
- **Settings > Keyboard...**: the Keyboard Setup dialog.
  - Lists every function (Enter, Clear, PA1-PA3, PF1-PF24, Reset, Erase EOF,
    Field Mark, Dup, cursor movement, clipboard, special characters such as
    ¢ and ¬) with the keys assigned to each.
  - Click **New key**, press any key or combination, and click **Assign**. If
    that key already does something else, or Windows uses it (Alt+F4,
    Alt+Tab, ...), you're asked before anything changes.
  - Ctrl, Alt and Shift keys can be mapped **on their own**, with left and
    right kept apart: e.g. Right Ctrl = Enter, Left Ctrl = Reset. They fire
    when released, and only if no other key was pressed, so Ctrl+C still
    copies.
  - Numpad Enter is a separate key from the main Enter.
  - Two presets to start from: **Classic 3270** (Right Ctrl and Numpad Enter
    send Enter, the Enter key is New Line) and **PC style** (Enter sends Enter).
  - **Your own presets**: set up your keys, click **Save as...** and give
    them a name. Saved presets appear in the Preset box next to the built-in
    ones; pick one and click **Load** to switch, or **Delete** to remove it.
    They're stored one file per preset in `%APPDATA%\tn3270win\keymaps`, so
    you can copy them to another PC or share them.
  - Enter/PF/PA keys don't auto-repeat when held; cursor and editing keys do.
- Mappings are saved to `%APPDATA%\tn3270win\keymap.kmp`, a plain text file
  you can edit or share:

  ```ini
  name = My keys

  [keys]
  Right Ctrl = Enter
  Enter = NewLine
  Left Ctrl = Reset
  Shift+F1 = PF13
  Alt+F1 = PA1
  Ctrl+End = EraseEOF
  Alt+6 = Text "¬"
  ```

## Building

Needs Rust (stable) on Windows with the MSVC toolchain. There are no
dependencies outside the Rust standard library.

```
cargo test --workspace
cargo run --release -p tn3270win
```

Every push to GitHub runs tests and builds `tn3270win.exe` on Windows (see
the Actions tab, artifact `tn3270win-windows-x64`).

## Layout

| Path | What |
|---|---|
| `crates/tn3270` | Protocol engine: Telnet/TN3270E negotiation, 3270 data stream, screen and fields, EBCDIC code pages, background TCP connection. Tested against a fake host. |
| `crates/keymap` | Platform-independent keyboard model: key chords, 3270 actions, presets, `.kmp` format, key-event dispatcher. Fully unit-tested. |
| `app/src/main.rs` | Main window, menus, screen drawing, keys |
| `app/src/connectdlg.rs` | Connect dialog |
| `app/src/keydlg.rs` | Keyboard Setup dialog |
| `app/src/capture.rs` | "Press a key" capture control |
| `app/src/prompt.rs` | Small "enter a name" dialog |
| `app/src/win32.rs` | Small hand-written Win32 bindings |

## Roadmap

1. **Keyboard mapping**. Later: a
   "show all keys" view, keyboard-picture view.
2. **3270 engine and live sessions**.
3. **Sessions, selection, screen sizes, view settings** (this milestone).
   Later: appearance settings (fonts, colours), TLS.
4. **Productivity**: macros, hotspots, IND$FILE file transfer, printing,
   multiple sessions.
