# tn3270win

A TN3270 terminal emulator for Windows, written in Rust, with a native Win32
interface: a real menu bar, standard dialogs and keyboard navigation. The
first priority is keyboard mapping.

## Status: milestone 1, keyboard mapping

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
| `crates/keymap` | Platform-independent keyboard model: key chords, 3270 actions, presets, `.kmp` format, key-event dispatcher. Fully unit-tested. |
| `app/src/main.rs` | Main window, menus, keyboard test mode |
| `app/src/keydlg.rs` | Keyboard Setup dialog |
| `app/src/capture.rs` | "Press a key" capture control |
| `app/src/win32.rs` | Small hand-written Win32 bindings |

## Roadmap

1. **Keyboard mapping** (this milestone). Next: import/export dialogs,
   per-session keymaps, a "show all keys" view, keyboard-picture view.
2. **3270 engine** (`crates/tn3270`): Telnet/TN3270E negotiation, TLS, the 3270
   data stream (fields, attributes, extended colours), EBCDIC code pages.
   Checked against x3270 and a Hercules/TK5 test host.
3. **Live sessions**: connect dialog and session profiles, cursor, insert
   mode, status line, copy/paste by field.
4. **Productivity**: macros, hotspots, IND$FILE file transfer, printing,
   multiple sessions.
