# EOS Daily-Driver Roadmap: 50 Autonomous Userspace Milestones

**Target Vision:** Windows 7 Aero Glass UX/UI + Full Built-in App Suite + Embedded Userspace Python Interpreter & Desktop Automation + `TEST_SUBJECT` Golden-Matrix Verification.

updating procudere :
updateMAX.py will be used to push update using this formula, follow it precisly:
Block formats in 'patch.txt' (every block must end with '### END'):

1.  Full file replacement / creation:

    ### FILE: path/to/file.qml

    <full file contents>
    ### END

2.  Inline Context-Anchored Edits (Works on .qml, .py, .rs, .toml, .ini, etc.):

    ### EDIT: ui/main.qml

    <<< FIND
    readonly property bool effectiveShowPreview: userShowPreview && (width >= 960 || height >= 760)
    ===
    readonly property bool effectiveShowPreview: userShowPreview && (width >= 800 || height >= 600)

    > > > <<< FIND
    > > > function endRightDrag() {

        isRightDragging = false

    # }

    function endRightDrag() {
    isRightDragging = false
    draggedFilePath = ""
    }

    > > >

    ### END
    - Supports multiple <<< FIND ... === ... >>> hunks inside a single ### EDIT block.
    - Indentation-insensitive matching: Even if your FIND or REPLACE block has 0 indent
      (or wrong base indent), the engine matches the exact relative structure and
      automatically re-indents your replacement to match the target file's exact level!
    - Uniqueness guarantee: Rejects the patch if FIND matches 0 times or >1 times.
    - Empty replacement (nothing between === and >>>) cleanly deletes the matched lines.

> **STRICT AGENT RULES:**
>
> 1. Execute milestones strictly in order (01 -> 50).
> 2. **VERIFY & GIT PUSH:** Every milestone requires an `[EOS_AUDIT]` marker and `TEST_SUBJECT` verification before.
> 3. After every major milestones you will push an update to "test_eos_audit.py" to test the newly added features.

- [x] **Milestone 01: Userspace `TEST_SUBJECT` Scanner & Checksum Verifier**
    - **ID:** `m01_test_subject_scanner`
    - **Goal:** Create `userspace/src/test_subject.rs` and wire it into `userspace/src/main.rs`. On desktop startup, scan `/EOS SHARE/TEST_SUBJECT`, compute CRC32 and byte sizes of discovered test files, and emit `[EOS_AUDIT]` marker `TEST_SUBJECT_SCAN:PASS:<count>_files` via `crate::sdk::agent::audit_log`.
    - **Files:** `userspace/src/test_subject.rs`, `userspace/src/main.rs`, `userspace/src/sdk.rs`
    - **Expect:** `TEST_SUBJECT_SCAN:PASS:`
    - **Steps:** `[]`

- [x] **Milestone 02: Image Decoder Golden Matrix Validator (PNG/JPEG/QOI)**
    - **ID:** `m02_image_matrix_validator`
    - **Goal:** Extend `userspace/src/test_subject.rs` to decode `/EOS SHARE/TEST_SUBJECT/color_bars_16x16.png` using `crate::png`, verify dimensions `16x16` and top-left/bottom-right pixel ARGB values against the golden matrix, and emit `IMAGE_MATRIX_VERIFY:PASS:16x16_ARGB_OK`.
    - **Files:** `userspace/src/test_subject.rs`, `userspace/src/png.rs`
    - **Expect:** `IMAGE_MATRIX_VERIFY:PASS:16x16_ARGB_OK`
    - **Steps:** `[]`

- [x] **Milestone 03: TrueType Font Metrics & Text Bounding-Box Calibrator**
    - **ID:** `m03_font_metrics_calibrator`
    - **Goal:** Add a font measurement calibration test in `userspace/src/test_subject.rs` that measures standard pangram strings from `TEST_SUBJECT` across caption, body, and title sizes, logs rendering time in microseconds for tuning, and emits `FONT_CALIBRATION:PASS:Metrics_Verified`.
    - **Files:** `userspace/src/test_subject.rs`, `userspace/src/framework/canvas.rs`
    - **Expect:** `FONT_CALIBRATION:PASS:Metrics_Verified`
    - **Steps:** `[]`

- [x] **Milestone 04: Compositor Frame-Time & Damage-Region Benchmark**
    - **ID:** `m04_compositor_benchmark`
    - **Goal:** Instrument `userspace/src/framework/perf_log.rs` and `userspace/src/framework/app.rs` to track 60 FPS frame budget, dirty rectangle merge efficiency, and emit `COMPOSITOR_TUNE:PASS:FrameBudget_OK` after the first 5 rendered frames.
    - **Files:** `userspace/src/framework/perf_log.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `COMPOSITOR_TUNE:PASS:FrameBudget_OK`
    - **Steps:** `[]`

- [x] **Milestone 05: Automated `TEST_SUBJECT` Report Exporter (`eos_output.json`)**
    - **ID:** `m05_test_report_exporter`
    - **Goal:** In `userspace/src/test_subject.rs`, serialize all Userspace diagnostic matrices (decoded pixel samples, file CRC32s, font metrics, and frame timings) into `/EOS SHARE/TEST_SUBJECT/eos_output.json` and emit `TEST_REPORT_EXPORT:PASS:eos_output.json`.
    - **Files:** `userspace/src/test_subject.rs`, `userspace/src/main.rs`
    - **Expect:** `TEST_REPORT_EXPORT:PASS:eos_output.json`
    - **Steps:** `[]`

---

## Phase 2: Windows 7 Aero Glass Desktop & Shell Transformation (06–15)

- [ ] **Milestone 06: Windows 7 Aero Glass Theme Engine & Color Palette**
    - **ID:** `m06_win7_aero_theme`
    - **Goal:** Upgrade `userspace/src/framework/theme.rs` with Windows 7 Aero Glass visual tokens (translucent sky-blue glass chrome, inner white frost border highlights, Segoe UI metric scaling, active/inactive window glow colors) and emit `WIN7_THEME:PASS:AeroGlass_Tokens_Ready`.
    - **Files:** `userspace/src/framework/theme.rs`, `userspace/src/framework/mod.rs`
    - **Expect:** `WIN7_THEME:PASS:AeroGlass_Tokens_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 07: Windows 7 Bottom Superbar (Taskbar) & Start Orb**
    - **ID:** `m07_win7_superbar_taskbar`
    - **Goal:** Transform the bottom dock in `userspace/src/framework/app.rs` into a full-width Windows 7 Aero Superbar (`40pt` height) featuring the iconic circular Start Orb on the left, pinned/running application glass buttons with active indicators in the center-left, and emit `WIN7_TASKBAR:PASS:Superbar_Rendered`.
    - **Files:** `userspace/src/framework/app.rs`, `userspace/src/framework/theme.rs`
    - **Expect:** `WIN7_TASKBAR:PASS:Superbar_Rendered`
    - **Steps:** `[]`

- [ ] **Milestone 08: Windows 7 System Tray, Clock/Calendar Flyout & "Aero Peek" Show Desktop Strip**
    - **ID:** `m08_win7_systray_peek`
    - **Goal:** Add the right-side Windows 7 System Tray in `userspace/src/framework/app.rs` with network/volume status icons, two-line time & date display, and the far-right `12pt` "Show Desktop" Aero Peek vertical rectangle button. Emit `WIN7_SYSTRAY:PASS:Clock_And_Peek_Ready`.
    - **Files:** `userspace/src/framework/app.rs`, `userspace/src/framework/menu.rs`
    - **Expect:** `WIN7_SYSTRAY:PASS:Clock_And_Peek_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 09: Windows 7 Two-Column Start Menu Popup & Live Search Box**
    - **ID:** `m09_win7_start_menu`
    - **Goal:** Implement the authentic Windows 7 two-column Start Menu popup in `userspace/src/framework/menu.rs` and `userspace/src/framework/app.rs` (Left white pane: pinned/recent programs + "Search programs and files" input box; Right dark-glass pane: Computer, Documents, Control Panel, Run, Shut Down button). Emit `WIN7_STARTMENU:PASS:TwoColumn_Ready`.
    - **Files:** `userspace/src/framework/menu.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `WIN7_STARTMENU:PASS:TwoColumn_Ready`
    - **Steps:** `[{"move":[-900,500]},{"click":true},{"wait":0.5}]`

- [ ] **Milestone 10: Windows 7 Window Chrome & Caption Buttons (Min / Max / Red-Glow Close)**
    - **ID:** `m10_win7_window_chrome`
    - **Goal:** Update `WindowFrame` in `userspace/src/framework/widget.rs` to render Windows 7 top-right rectangular grouped caption buttons (Minimize `_`, Maximize/Restore `□`, and wide red-glow Close `X` button on the right, instead of left traffic lights) with glass reflection stripes. Emit `WIN7_WINDOW_CHROME:PASS:AeroButtons_Active`.
    - **Files:** `userspace/src/framework/widget.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `WIN7_WINDOW_CHROME:PASS:AeroButtons_Active`
    - **Steps:** `[]`

- [ ] **Milestone 11: Aero Snap (Left/Right Half-Screen & Top Maximize Preview)**
    - **ID:** `m11_win7_aero_snap`
    - **Goal:** Enhance window dragging in `userspace/src/framework/app.rs` and `userspace/src/framework/widget.rs` with translucent glass Aero Snap preview outlines when dragging a window to the left edge, right edge, or top edge of the screen, plus `Win+Left` / `Win+Right` keyboard snapping. Emit `WIN7_AEROSNAP:PASS:SnapEngine_Ready`.
    - **Files:** `userspace/src/framework/app.rs`, `userspace/src/framework/widget.rs`
    - **Expect:** `WIN7_AEROSNAP:PASS:SnapEngine_Ready`
    - **Steps:** `[{"move":[-400,0]},{"wait":0.3}]`

- [ ] **Milestone 12: Desktop Icons Grid (Computer, Recycle Bin, User Files, Python IDE)**
    - **ID:** `m12_win7_desktop_icons`
    - **Goal:** Add an interactive Desktop Icon Grid layer on the wallpaper in `userspace/src/framework/app.rs` with rubber-band selection box, click-to-select, and double-click to launch `Computer`, `Recycle Bin`, `TEST_SUBJECT`, and `Command Prompt`. Emit `WIN7_DESKTOP_ICONS:PASS:Grid_Rendered`.
    - **Files:** `userspace/src/framework/app.rs`, `userspace/src/framework/vector.rs`
    - **Expect:** `WIN7_DESKTOP_ICONS:PASS:Grid_Rendered`
    - **Steps:** `[]`

- [ ] **Milestone 13: Desktop Right-Click Context Menu (View, Sort By, Refresh, New, Personalize)**
    - **ID:** `m13_win7_context_menu`
    - **Goal:** Implement a cascading Windows 7 right-click context menu in `userspace/src/framework/app.rs` supporting Desktop Refresh, Icon Sorting, New Folder/Text Document/Python Script creation, and opening Personalize/Settings. Emit `WIN7_CONTEXT_MENU:PASS:Handler_Registered`.
    - **Files:** `userspace/src/framework/app.rs`, `userspace/src/framework/menu.rs`
    - **Expect:** `WIN7_CONTEXT_MENU:PASS:Handler_Registered`
    - **Steps:** `[]`

- [ ] **Milestone 14: Alt+Tab Aero Live Window Switcher Overlay**
    - **ID:** `m14_win7_alt_tab`
    - **Goal:** Add a centered glass `Alt+Tab` window switcher overlay in `userspace/src/framework/app.rs` displaying icons and titles of all open windows and cycling focus cleanly. Emit `WIN7_ALTTAB:PASS:Switcher_Ready`.
    - **Files:** `userspace/src/framework/app.rs`
    - **Expect:** `WIN7_ALTTAB:PASS:Switcher_Ready`
    - **Steps:** `[{"key":"tab"},{"wait":0.4}]`

- [ ] **Milestone 15: Taskbar Live Thumbnail Hover Previews & Window Minimize/Restore**
    - **ID:** `m15_win7_taskbar_previews`
    - **Goal:** Support minimizing windows to the Superbar in `userspace/src/framework/app.rs` and rendering a Windows 7 Aero popup preview card when hovering over taskbar buttons. Emit `WIN7_TASKBAR_PREVIEW:PASS:HoverCard_Ready`.
    - **Files:** `userspace/src/framework/app.rs`, `userspace/src/framework/widget.rs`
    - **Expect:** `WIN7_TASKBAR_PREVIEW:PASS:HoverCard_Ready`
    - **Steps:** `[]`

---

## Phase 3: Embedded Userspace Python Interpreter & Automation Engine (16–25)

- [ ] **Milestone 16: Userspace Python Lexer & Tokenizer (`eos_python`)**
    - **ID:** `m16_python_lexer`
    - **Goal:** Create `userspace/src/python/mod.rs` and `userspace/src/python/lexer.rs` implementing a clean Python 3 subset tokenizer (`INDENT`, `DEDENT`, identifiers, numbers, strings, operators, keywords `def, if, elif, else, for, while, in, return, import, True, False, None, and, or, not`). Verify tokenization on a sample script and emit `PYTHON_LEXER:PASS:Tokens_Verified`.
    - **Files:** `userspace/src/python/mod.rs`, `userspace/src/python/lexer.rs`, `userspace/src/main.rs`
    - **Expect:** `PYTHON_LEXER:PASS:Tokens_Verified`
    - **Steps:** `[]`

- [ ] **Milestone 17: Python AST Parser & Expression Evaluator**
    - **ID:** `m17_python_ast_parser`
    - **Goal:** Create `userspace/src/python/parser.rs` and `userspace/src/python/ast.rs` supporting arithmetic/logical expressions, list/dict literals, indexing, function calls, and assignment statements. Emit `PYTHON_PARSER:PASS:AST_Ready`.
    - **Files:** `userspace/src/python/mod.rs`, `userspace/src/python/ast.rs`, `userspace/src/python/parser.rs`
    - **Expect:** `PYTHON_PARSER:PASS:AST_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 18: Python Runtime VM: Control Flow, Functions & Closures**
    - **ID:** `m18_python_runtime_vm`
    - **Goal:** Create `userspace/src/python/eval.rs` executing `if/elif/else`, `while`, `for x in range(...)`, `def` functions with recursion, local/global scopes, and string/list/dict methods. Run `TEST_SUBJECT` math verification and emit `PYTHON_VM:PASS:Execution_Verified`.
    - **Files:** `userspace/src/python/mod.rs`, `userspace/src/python/eval.rs`, `userspace/src/test_subject.rs`
    - **Expect:** `PYTHON_VM:PASS:Execution_Verified`
    - **Steps:** `[]`

- [ ] **Milestone 19: Python Built-in Standard Modules (`os`, `sys`, `math`, `json`, `time`)**
    - **ID:** `m19_python_stdlib`
    - **Goal:** Add built-in module bindings in `userspace/src/python/stdlib.rs` for `os.listdir`, `os.read_text`, `os.write_text`, `os.stat`, `json.loads`, `json.dumps`, `math.*`, and `time.ticks()`. Emit `PYTHON_STDLIB:PASS:Modules_Bound`.
    - **Files:** `userspace/src/python/mod.rs`, `userspace/src/python/stdlib.rs`, `userspace/src/python/eval.rs`
    - **Expect:** `PYTHON_STDLIB:PASS:Modules_Bound`
    - **Steps:** `[]`

- [ ] **Milestone 20: Python `eos_auto` Desktop & UI Automation Bridge**
    - **ID:** `m20_python_eos_auto_bridge`
    - **Goal:** Expose an `eos_auto` module in `userspace/src/python/stdlib.rs` allowing Python scripts to queue desktop actions (`eos_auto.spawn("Calculator")`, `eos_auto.click(x, y)`, `eos_auto.type_text("123")`, `eos_auto.list_windows()`, `eos_auto.audit("msg")`). Emit `PYTHON_AUTOMATION:PASS:Bridge_Ready`.
    - **Files:** `userspace/src/python/stdlib.rs`, `userspace/src/python/mod.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `PYTHON_AUTOMATION:PASS:Bridge_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 21: Interactive Python REPL & Script Execution in Command Prompt (`Terminal`)**
    - **ID:** `m21_terminal_python_repl`
    - **Goal:** Upgrade `userspace/src/apps/terminal.rs` so typing `python` enters an interactive multi-line REPL, and `python <file.py>` executes any Python script from `/EOS SHARE` or `/RootFS` directly using `crate::python`. Emit `TERMINAL_PYTHON:PASS:REPL_And_Runner_Ready`.
    - **Files:** `userspace/src/apps/terminal.rs`, `userspace/src/python/mod.rs`
    - **Expect:** `TERMINAL_PYTHON:PASS:REPL_And_Runner_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 22: EOS Python IDE & Automation Studio Application**
    - **ID:** `m22_python_automation_studio_app`
    - **Goal:** Create `userspace/src/apps/python_studio.rs` providing a Windows 7 styled Python IDE & Automation Recorder with code editor, "Run Script (F5)" button, live output console, and preset automation templates. Register it in `apps/mod.rs` and `FrameworkApp`. Emit `APP_PYSTUDIO:PASS:Registered`.
    - **Files:** `userspace/src/apps/python_studio.rs`, `userspace/src/apps/mod.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `APP_PYSTUDIO:PASS:Registered`
    - **Steps:** `[]`

- [ ] **Milestone 23: Scheduled Background Automation Runner & Watchdog Tasks**
    - **ID:** `m23_automation_scheduler`
    - **Goal:** Add a non-blocking periodic task runner in `userspace/src/python/mod.rs` invoked from `FrameworkApp::run_loop` that can run scheduled Python automation hooks (e.g., folder organizer, system telemetry logger) without dropping UI FPS. Emit `AUTO_SCHEDULER:PASS:Hook_Active`.
    - **Files:** `userspace/src/python/mod.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `AUTO_SCHEDULER:PASS:Hook_Active`
    - **Steps:** `[]`

- [ ] **Milestone 24: Batch File & Media Processing Automation via Python**
    - **ID:** `m24_python_media_batch`
    - **Goal:** Add `eos_media` functions to `userspace/src/python/stdlib.rs` (`inspect_image(path)`, `batch_rename(dir, pattern)`, `csv_summary(path)`) and verify against `/EOS SHARE/TEST_SUBJECT`. Emit `PYTHON_MEDIA_BATCH:PASS:Verified`.
    - **Files:** `userspace/src/python/stdlib.rs`, `userspace/src/test_subject.rs`
    - **Expect:** `PYTHON_MEDIA_BATCH:PASS:Verified`
    - **Steps:** `[]`

- [ ] **Milestone 25: Full `TEST_SUBJECT/python_suite` End-to-End Verification**
    - **ID:** `m25_python_suite_verification`
    - **Goal:** Execute all `.py` test scripts inside `/EOS SHARE/TEST_SUBJECT` during `test_subject::run_all_verifications()`, compare outputs with golden expectations, write results to `eos_output.json`, and emit `PYTHON_SUITE_AUDIT:PASS:All_Scripts_Verified`.
    - **Files:** `userspace/src/test_subject.rs`, `userspace/src/python/mod.rs`
    - **Expect:** `PYTHON_SUITE_AUDIT:PASS:All_Scripts_Verified`
    - **Steps:** `[]`

---

## Phase 4: Windows 7 Core Built-in Applications Suite (26–38)

- [ ] **Milestone 26: Windows Explorer (`Finder` Upgrade) — Breadcrumb Address Bar & Navigation Pane**
    - **ID:** `m26_win7_explorer_breadcrumbs`
    - **Goal:** Upgrade `userspace/src/apps/finder.rs` into **Windows Explorer** featuring Back/Forward Aero round buttons, a clickable breadcrumb path bar (`Computer > EOS SHARE > TEST_SUBJECT`), quick search box, and collapsible left Navigation Pane (`Favorites, Libraries, Computer, Network`). Emit `WIN7_EXPLORER:PASS:Breadcrumbs_Ready`.
    - **Files:** `userspace/src/apps/finder.rs`
    - **Expect:** `WIN7_EXPLORER:PASS:Breadcrumbs_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 27: Windows Explorer — Details View, Column Sorting & Bottom Details Pane**
    - **ID:** `m27_win7_explorer_details_pane`
    - **Goal:** Add Details View (Name, Date Modified, Type, Size columns with click-to-sort), Icons/Tiles view toggle, and the Windows 7 bottom Details Pane showing selected file metadata and thumbnail in `userspace/src/apps/finder.rs`. Emit `WIN7_EXPLORER_DETAILS:PASS:Columns_And_Pane_Ready`.
    - **Files:** `userspace/src/apps/finder.rs`
    - **Expect:** `WIN7_EXPLORER_DETAILS:PASS:Columns_And_Pane_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 28: Windows Explorer — File Operations (Copy, Paste, Rename, Delete to Recycle Bin)**
    - **ID:** `m28_win7_explorer_file_ops`
    - **Goal:** Implement Copy, Cut, Paste, New Folder, Rename, and Delete (moving items to a virtual Recycle Bin list) inside `userspace/src/apps/finder.rs` toolbar and context menu. Emit `WIN7_EXPLORER_OPS:PASS:Clipboard_And_Recycle_Ready`.
    - **Files:** `userspace/src/apps/finder.rs`
    - **Expect:** `WIN7_EXPLORER_OPS:PASS:Clipboard_And_Recycle_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 29: Windows 7 Calculator (Standard, Scientific & Programmer Hex/Bin Modes)**
    - **ID:** `m29_win7_calculator`
    - **Goal:** Create `userspace/src/apps/calculator.rs` styled after Windows 7 Calculator with LCD gradient display, history tape, mode switcher (`Standard`, `Scientific`, `Programmer` with `HEX/DEC/OCT/BIN` bitwise ops), and register it in `FrameworkApp`. Emit `APP_CALC:PASS:Registered`.
    - **Files:** `userspace/src/apps/calculator.rs`, `userspace/src/apps/mod.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `APP_CALC:PASS:Registered`
    - **Steps:** `[]`

- [ ] **Milestone 30: Windows 7 Paint (Ribbon Toolbar, Brushes, Shapes, Flood Fill & Save BMP/PNG)**
    - **ID:** `m30_win7_paint`
    - **Goal:** Create or upgrade `userspace/src/apps/paint.rs` with a Windows 7 Ribbon UI containing Pencil, Brush, Flood Fill bucket, Line, Rectangle, Circle, Eraser, stroke thickness selector, 20-color palette, and Save to `/EOS SHARE`. Emit `APP_PAINT:PASS:Ribbon_Ready`.
    - **Files:** `userspace/src/apps/paint.rs`, `userspace/src/apps/mod.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `APP_PAINT:PASS:Ribbon_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 31: Windows 7 Notepad & WordPad (Find/Replace, Line Numbers, Font Formatting)**
    - **ID:** `m31_win7_notepad_wordpad`
    - **Goal:** Upgrade `userspace/src/apps/textedit.rs` with a Find & Replace bar (`Ctrl+F`), Undo/Redo stack (`Ctrl+Z`/`Ctrl+Y`), status bar (`Ln, Col, UTF-8`), word-wrap toggle, and rich heading/color formatting modes. Emit `WIN7_NOTEPAD:PASS:FindReplace_Undo_Ready`.
    - **Files:** `userspace/src/apps/textedit.rs`
    - **Expect:** `WIN7_NOTEPAD:PASS:FindReplace_Undo_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 32: Windows Photo Viewer (`Preview` Upgrade) — Zoom, Pan, Rotate & Slideshow**
    - **ID:** `m32_win7_photo_viewer`
    - **Goal:** Upgrade `userspace/src/apps/preview.rs` into **Windows Photo Viewer** with the classic bottom glossy control bar (Previous/Next image in folder, Zoom slider, Fit-to-Window, Rotate 90° CW/CCW, Delete). Emit `WIN7_PHOTOVIEWER:PASS:Controls_Ready`.
    - **Files:** `userspace/src/apps/preview.rs`
    - **Expect:** `WIN7_PHOTOVIEWER:PASS:Controls_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 33: Windows Media Player 12 (Audio/Video Playback, Playlist & Visualizer)**
    - **ID:** `m33_win7_media_player`
    - **Goal:** Create `userspace/src/apps/media_player.rs` featuring Windows Media Player 12 "Now Playing" & "Library" views, animated audio spectrum bars / Alchemy visualizer, timeline seek bar, playlist loader for `TEST_SUBJECT` media, and transport controls. Register in `FrameworkApp`. Emit `APP_WMP12:PASS:Registered`.
    - **Files:** `userspace/src/apps/media_player.rs`, `userspace/src/apps/mod.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `APP_WMP12:PASS:Registered`
    - **Steps:** `[]`

- [ ] **Milestone 34: Windows Task Manager (`Activity Monitor` Upgrade) — Applications, Processes, Performance & Networking Tabs**
    - **ID:** `m34_win7_task_manager`
    - **Goal:** Upgrade `userspace/src/apps/activity_monitor.rs` into **Windows Task Manager** with 4 tabs (`Applications` with "End Task" / "New Task...", `Processes`, `Performance` with classic green-grid CPU & Memory history graphs, and `Networking`). Emit `WIN7_TASKMGR:PASS:FourTabs_Ready`.
    - **Files:** `userspace/src/apps/activity_monitor.rs`
    - **Expect:** `WIN7_TASKMGR:PASS:FourTabs_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 35: Windows 7 Control Panel & Personalization Hub (`Settings` Upgrade)**
    - **ID:** `m35_win7_control_panel`
    - **Goal:** Upgrade `userspace/src/apps/settings.rs` into **Control Panel** with Category View (`System and Security`, `Network and Internet`, `Appearance and Personalization`, `Clock, Language, and Region`), Aero glass color mixer, wallpaper picker, and DPI scaling selector. Emit `WIN7_CONTROLPANEL:PASS:Categories_Ready`.
    - **Files:** `userspace/src/apps/settings.rs`
    - **Expect:** `WIN7_CONTROLPANEL:PASS:Categories_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 36: Windows 7 Command Prompt & PowerShell Mode (`cmd.exe` Style + Piping)**
    - **ID:** `m36_win7_cmd_powershell`
    - **Goal:** Enhance `userspace/src/apps/terminal.rs` with classic Windows 7 `cmd.exe` (`dir`, `cd`, `type`, `copy`, `del`, `cls`, `ipconfig`, `ping`, `tasklist`, `systeminfo`, `ver`, `color`) alongside Unix aliases, command history (`Up/Down`), Tab auto-completion, and `|` piping. Emit `WIN7_CMD:PASS:ShellCommands_Ready`.
    - **Files:** `userspace/src/apps/terminal.rs`
    - **Expect:** `WIN7_CMD:PASS:ShellCommands_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 37: Windows 7 Sticky Notes & Desktop Gadgets (CPU Meter, Clock, Weather/Notes)**
    - **ID:** `m37_win7_sticky_gadgets`
    - **Goal:** Create `userspace/src/apps/sticky_notes.rs` and desktop gadget overlays in `userspace/src/framework/app.rs` (analog clock gadget, CPU/RAM dial meter gadget, and pastel yellow/blue/pink Sticky Notes persisted to disk). Emit `WIN7_GADGETS:PASS:Sticky_And_Meters_Ready`.
    - **Files:** `userspace/src/apps/sticky_notes.rs`, `userspace/src/apps/mod.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `WIN7_GADGETS:PASS:Sticky_And_Meters_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 38: Windows 7 Snipping Tool (Region Capture & Save to Disk)**
    - **ID:** `m38_win7_snipping_tool`
    - **Goal:** Create `userspace/src/apps/snipping_tool.rs` allowing users to capture the desktop or active window buffer, annotate with a red highlighter pen, and save as PNG/BMP to `/EOS SHARE`. Register in `FrameworkApp`. Emit `APP_SNIPPING:PASS:Registered`.
    - **Files:** `userspace/src/apps/snipping_tool.rs`, `userspace/src/apps/mod.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `APP_SNIPPING:PASS:Registered`
    - **Steps:** `[]`

---

## Phase 5: Networking, System Utilities, Games & Daily-Driver Polish (39–50)

- [ ] **Milestone 39: Internet Explorer 9 / EOS Browser — Tabs, Bookmarks, Local Docs & HTML5 Layout**
    - **ID:** `m39_win7_browser_tabs`
    - **Goal:** Upgrade `userspace/src/apps/browser.rs` with multi-tab browsing, bookmarks bar, local file viewer (`file:///EOS SHARE/...`), CSS box-model table/card rendering, and JSON API inspector. Emit `WIN7_BROWSER:PASS:MultiTab_Ready`.
    - **Files:** `userspace/src/apps/browser.rs`
    - **Expect:** `WIN7_BROWSER:PASS:MultiTab_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 40: Windows 7 Run Dialog (`Win+R`) & Global Shortcut Dispatcher**
    - **ID:** `m40_win7_run_dialog`
    - **Goal:** Implement the classic Windows 7 `Run` dialog (`Win+R`) in `userspace/src/framework/app.rs` accepting `cmd`, `calc`, `mspaint`, `notepad`, `explorer`, `taskmgr`, `control`, `python`, or any path/script to launch immediately. Emit `WIN7_RUN_DIALOG:PASS:Dispatcher_Ready`.
    - **Files:** `userspace/src/framework/app.rs`
    - **Expect:** `WIN7_RUN_DIALOG:PASS:Dispatcher_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 41: System-Wide Clipboard Manager & Cross-App Copy/Paste (`Ctrl+C` / `Ctrl+V`)**
    - **ID:** `m41_system_clipboard`
    - **Goal:** Add a shared Userspace Clipboard service in `userspace/src/framework/mod.rs` supporting text and file-path payloads across `Notepad`, `Command Prompt`, `Explorer`, `Python Studio`, and `Browser`. Emit `SYSTEM_CLIPBOARD:PASS:CrossApp_Ready`.
    - **Files:** `userspace/src/framework/mod.rs`, `userspace/src/framework/app.rs`, `userspace/src/apps/textedit.rs`, `userspace/src/apps/terminal.rs`
    - **Expect:** `SYSTEM_CLIPBOARD:PASS:CrossApp_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 42: Windows 7 Action Center & Balloon Toast Notifications**
    - **ID:** `m42_win7_notifications`
    - **Goal:** Implement slide-up Aero balloon toast notifications above the System Tray in `userspace/src/framework/app.rs` with an Action Center history list for system events, file saves, and Python automation alerts. Emit `WIN7_TOAST:PASS:ActionCenter_Ready`.
    - **Files:** `userspace/src/framework/app.rs`
    - **Expect:** `WIN7_TOAST:PASS:ActionCenter_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 43: Windows 7 Minesweeper & Solitaire Classic Games**
    - **ID:** `m43_win7_games_minesweeper`
    - **Goal:** Create `userspace/src/apps/minesweeper.rs` implementing classic Windows 7 Minesweeper (Beginner/Intermediate grids, flood-fill zero reveal, flag counter, timer) and register in `FrameworkApp`. Emit `APP_MINESWEEPER:PASS:Registered`.
    - **Files:** `userspace/src/apps/minesweeper.rs`, `userspace/src/apps/mod.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `APP_MINESWEEPER:PASS:Registered`
    - **Steps:** `[]`

- [ ] **Milestone 44: Archive Manager (`.tar` / `.zip` Inspector & Extractor in Explorer)**
    - **ID:** `m44_archive_extractor`
    - **Goal:** Add built-in `.tar` and `.zip` (stored/deflate) archive inspection and extraction inside `userspace/src/apps/finder.rs` and `userspace/src/python/stdlib.rs`. Verify on a test archive and emit `ARCHIVE_MGR:PASS:TarZip_Ready`.
    - **Files:** `userspace/src/apps/finder.rs`, `userspace/src/python/stdlib.rs`
    - **Expect:** `ARCHIVE_MGR:PASS:TarZip_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 45: System Registry / Configuration Store (`eos_reg.ini`) & Session Persistence**
    - **ID:** `m45_session_persistence`
    - **Goal:** Persist open window positions, pinned taskbar items, theme color, desktop icon positions, and recent files to `/home/march/.config/settings.ini` (and `/EOS SHARE/eos_session.json`) and restore them cleanly on startup. Emit `SESSION_PERSIST:PASS:State_Restored`.
    - **Files:** `userspace/src/framework/app.rs`, `userspace/src/apps/settings.rs`
    - **Expect:** `SESSION_PERSIST:PASS:State_Restored`
    - **Steps:** `[]`

- [ ] **Milestone 46: Keyboard Navigation, Focus Rings & Accessibility Shortcuts**
    - **ID:** `m46_keyboard_accessibility`
    - **Goal:** Add `Tab` / `Shift+Tab` control focus traversal, `Enter`/`Space` activation, `Alt+F4` close active window, and `Win+D` toggle show desktop in `userspace/src/framework/app.rs`. Emit `A11Y_SHORTCUTS:PASS:FocusTraversal_Ready`.
    - **Files:** `userspace/src/framework/app.rs`, `userspace/src/framework/controls.rs`
    - **Expect:** `A11Y_SHORTCUTS:PASS:FocusTraversal_Ready`
    - **Steps:** `[]`

- [ ] **Milestone 47: Subpixel Text AA Tuning & SIMD-Style Software Blit Optimization**
    - **ID:** `m47_render_pipeline_tuning`
    - **Goal:** Optimize `userspace/src/framework/canvas.rs` and `userspace/src/framework/compositor.rs` with glyph raster caching, fast 64-bit pair pixel fills, and zero-allocation damage blitting. Benchmark against `TEST_SUBJECT` and emit `RENDER_TUNE:PASS:GlyphCache_And_FastBlit`.
    - **Files:** `userspace/src/framework/canvas.rs`, `userspace/src/framework/compositor.rs`
    - **Expect:** `RENDER_TUNE:PASS:GlyphCache_And_FastBlit`
    - **Steps:** `[]`

- [ ] **Milestone 48: Automated Self-Healing Watchdog & Crash Recovery Dialog**
    - **ID:** `m48_app_crash_recovery`
    - **Goal:** Add state-isolation guards around widget event handlers and Python script execution so any runtime error displays a Windows 7 "Program has stopped working — Restart the program" dialog without crashing the desktop shell. Emit `WATCHDOG_GUARD:PASS:FaultIsolation_Active`.
    - **Files:** `userspace/src/framework/app.rs`, `userspace/src/python/eval.rs`
    - **Expect:** `WATCHDOG_GUARD:PASS:FaultIsolation_Active`
    - **Steps:** `[]`

- [ ] **Milestone 49: Comprehensive UI Stress Test Macro (All Apps Open, Snap, Python Automation)**
    - **ID:** `m49_full_ui_stress_macro`
    - **Goal:** Add a built-in automated stress-test routine in `userspace/src/test_subject.rs` that programmatically launches Explorer, Calculator, Paint, Notepad, Media Player, Task Manager, and Python Studio, runs an automation script, verifies zero memory leaks or frame drops, and emits `STRESS_TEST_ALL_APPS:PASS:ZeroFailures`.
    - **Files:** `userspace/src/test_subject.rs`, `userspace/src/framework/app.rs`
    - **Expect:** `STRESS_TEST_ALL_APPS:PASS:ZeroFailures`
    - **Steps:** `[]`

- [ ] **Milestone 50: Windows 7 Ultimate Daily-Driver Golden Certification & Final Benchmark**
    - **ID:** `m50_daily_driver_certification`
    - **Goal:** Complete final polish of Start Menu, Aero Glass Taskbar, Python Automation Engine, and all `TEST_SUBJECT` golden matrix checks. Output the final certification report to `/EOS SHARE/TEST_SUBJECT/eos_certification.json` and emit `EOS_WIN7_DAILY_DRIVER:PASS:CERTIFIED_50_OF_50`.
    - **Files:** `userspace/src/test_subject.rs`, `userspace/src/framework/app.rs`, `userspace/src/main.rs`
    - **Expect:** `EOS_WIN7_DAILY_DRIVER:PASS:CERTIFIED_50_OF_50`
    - **Steps:** `[]`
