# EOS Operating System — Engineering Roadmap (32 Milestones)

**Core Philosophy:**

- **100% Upstream Framework GUI:** Every single pixel, desktop shell element, taskbar, start menu, context menu, window frame, dialog, and app widget MUST be built using `userspace/src/framework/` primitives (`Canvas`, `ContainerWidget`, `ImageWidget`, `ContextMenu`, `Theme`, `Animator`). Zero ad-hoc drawing in apps or shell.
- **Single-User Root Sovereignty:** No `hero` or `sudo` wrappers. The system belongs exclusively to the owner (`root` at all times, full read/write access across `RootFS`, `EOS SHARE`, and hardware).
- **TTY Console Switch (`Ctrl+Alt+F2` / `Ctrl+Alt+F1`):** Instant switching between the Ring 3 Graphical Compositor (`tty1`) and a full-screen Kernel/Userspace Root Shell (`tty2`) for low-level debugging and recovery.
- **Bilingual RTL/LTR & Multi-Font Engine:** Native Arabic contextual shaping (isolated/initial/medial/final forms), BiDi text reordering, RTL layout mirroring, and runtime font switching in `Theme`.
- **Static CPython Integration:** Embedding `libpython3.12.a` directly into Userspace (alongside static `FFmpeg`) for real Python scripting and OS automation.

**Updating Procedure (`updateMAX.py`):**
Every block in `patch.txt` must end with `### END`:

1. Full file replacement / creation:

    ### FILE: path/to/file.rs

    <full file contents>
    ### END

    Context-anchored inline edit:

    ### EDIT: path/to/file.rs

    <<< FIND
    <exact lines>
    ===
    <replacement lines>

    > > >

    ### END

STRICT EXECUTION RULES:

        Execute milestones in order (M01 -> M32).

        Every milestone MUST emit its [EOS_AUDIT] marker and pass test_eos_audit.py before advancing.

        Keep test_eos_audit.py updated as new subsystems come online.

Phase 1: Completed Foundation (Verified & Active)

    [x] Milestone 01: Userspace TEST_SUBJECT Scanner & Checksum Verifier

        ID: m01_test_subject_scanner

        Goal: Scan /EOS SHARE/TEST_SUBJECT, compute CRC32 and byte sizes, and emit TEST_SUBJECT_SCAN:PASS:<count>_files.

        Files: userspace/src/test_subject.rs, userspace/src/main.rs, userspace/src/sdk.rs

        Expect: TEST_SUBJECT_SCAN:PASS:

        Steps: []

    [x] Milestone 02: Image Decoder Golden Matrix Validator (PNG/JPEG)

        ID: m02_image_matrix_validator

        Goal: Decode /EOS SHARE/TEST_SUBJECT/color_bars_16x16.png via Syscall 510 / png.rs and verify ARGB pixels against golden matrix.

        Files: userspace/src/test_subject.rs, userspace/src/png.rs

        Expect: IMAGE_MATRIX_VERIFY:PASS:16x16_ARGB_OK

        Steps: []

    [x] Milestone 03: TrueType Font Metrics & Text Bounding-Box Calibrator

        ID: m03_font_metrics_calibrator

        Goal: Measure pangram strings across caption, body, and title sizes and emit FONT_CALIBRATION:PASS:Metrics_Verified.

        Files: userspace/src/test_subject.rs, userspace/src/framework/canvas.rs

        Expect: FONT_CALIBRATION:PASS:Metrics_Verified

        Steps: []

    [x] Milestone 04: Compositor Frame-Time & Damage-Region Benchmark

        ID: m04_compositor_benchmark

        Goal: Track 60 FPS frame budget and dirty rectangle merge efficiency in perf_log.rs and app.rs.

        Files: userspace/src/framework/perf_log.rs, userspace/src/framework/app.rs

        Expect: COMPOSITOR_TUNE:PASS:FrameBudget_OK

        Steps: []

    [x] Milestone 05: Automated TEST_SUBJECT Report Exporter (eos_output.json)

        ID: m05_test_report_exporter

        Goal: Export Userspace diagnostic matrices into /EOS SHARE/TEST_SUBJECT/eos_output.json.

        Files: userspace/src/test_subject.rs, userspace/src/main.rs

        Expect: TEST_REPORT_EXPORT:PASS:eos_output.json

        Steps: []

Phase 2: 100% Framework Shell Unification, Root Sovereignty & TTY Switch (M06–M11)

    [x] Milestone 06: Pure Root Mode & Removal of hero / Permission Restrictions

        ID: m06_pure_root_sovereignty

        Goal: Remove hero command prefix and all permission checks from userspace/src/apps/terminal.rs and src/fs/mod.rs. Set default user to root@eos (uid=0, gid=0 in syscall.rs) with unrestricted read/write/delete access across RootFS, EOS SHARE, and system directories. Emit ROOT_SOVEREIGNTY:PASS:Unrestricted_Root.

        Files: userspace/src/apps/terminal.rs, src/fs/mod.rs, src/arch/x86_64/syscall.rs

        Expect: ROOT_SOVEREIGNTY:PASS:Unrestricted_Root

        Steps: []

    [x] Milestone 07: Ctrl+Alt+F2 TTY Root Console & Ctrl+Alt+F1 GUI Switcher

        ID: m07_tty_console_switch

        Goal: Implement virtual terminal switching (tty1 Graphical Compositor vs tty2 Full-Screen Root Text Console) in src/arch/x86_64/keyboard.rs, src/writer.rs, and userspace/src/framework/app.rs. Pressing Ctrl+Alt+F2 suspends GUI presentation and opens an interactive full-screen root shell directly on the framebuffer; Ctrl+Alt+F1 restores the GUI desktop seamlessly. Emit TTY_SWITCH:PASS:F1_GUI_F2_RootShell.

        Files: src/arch/x86_64/keyboard.rs, src/writer.rs, src/arch/x86_64/syscall.rs, userspace/src/framework/app.rs

        Expect: TTY_SWITCH:PASS:F1_GUI_F2_RootShell

        Steps: []

    [x] Milestone 08: 100% Framework-Driven Desktop Shell & 60 FPS Animations

        ID: m08_framework_shell_unification

        Goal: Refactor FrameworkApp (userspace/src/framework/app.rs) so the Taskbar, System Tray, Start Orb (icon.png), Desktop Icon Grid, and Auto-Hide slide animation are built strictly from Framework widgets (ContainerWidget, ImageWidget, ButtonWidget, Animator at 60 FPS) with persistent auto_hide in RootFS/dock.ini and unified Dark/Light StartMenu. Emit FRAMEWORK_SHELL:PASS:100Pct_Unified.

        Files: userspace/src/framework/app.rs, userspace/src/framework/menu.rs, userspace/src/framework/widget.rs

        Expect: FRAMEWORK_SHELL:PASS:100Pct_Unified

        Steps: []

    [x] Milestone 09: Multi-Font Manager & Font Family Switcher in Framework

        ID: m09_framework_font_manager

        Goal: Build userspace/src/framework/fonts.rs supporting multiple bundled/disk TrueType/OpenType fonts (Sans, Mono, Arabic) with per-role selection (ui_font, mono_font, arabic_font) in Theme and a live Font Selector in Settings. Ensures Terminal and Stats HUD use fixed-width Mono metrics without jitter. Emit FONT_MGR:PASS:MultiFont_Active.

        Files: userspace/src/framework/fonts.rs, userspace/src/framework/theme.rs, userspace/src/framework/canvas.rs, userspace/src/apps/settings.rs

        Expect: FONT_MGR:PASS:MultiFont_Active

        Steps: []

    [x] Milestone 10: Native Arabic Contextual Shaping & Bidirectional (RTL/LTR) Layout Engine

        ID: m10_rtl_and_arabic_shaping

        Goal: Implement Arabic Unicode contextual glyph shaping (connecting isolated, initial, medial, and final letter forms + Lam-Alef ligatures) and BiDi RTL text layout in userspace/src/framework/canvas.rs and layout.rs, plus LayoutDirection::Auto / RTL / LTR in Theme. Enables clean Arabic subtitles (.ass/.srt), filenames, and RTL UI mirroring. Emit RTL_ENGINE:PASS:Arabic_Shaping_Ready.

        Files: userspace/src/framework/canvas.rs, userspace/src/framework/layout.rs, userspace/src/framework/theme.rs

        Expect: RTL_ENGINE:PASS:Arabic_Shaping_Ready

        Steps: []

    [x] Milestone 11: Real Process argv / Intent Passing & Cross-Process Kernel Clipboard

        ID: m11_process_argv_and_clipboard

        Goal: Replace temporary .ini file handoffs (open_file.ini, finder_nav.ini) by passing real argv strings through sys_spawn_elf into isolated ELF processes, and add kernel-backed sys_clipboard_set / sys_clipboard_get syscalls (545/546) so Ctrl+C / Ctrl+V works seamlessly across Terminal, TextEdit, Finder, and Browser. Emit IPC_CLIPBOARD_ARGV:PASS:Verified.

        Files: src/arch/x86_64/syscall.rs, src/task/mod.rs, userspace/src/syscall.rs, userspace/src/main.rs, userspace/src/apps/finder.rs

        Expect: IPC_CLIPBOARD_ARGV:PASS:Verified

        Steps: []

Phase 3: Developer Diagnostics, Framework Controls & Core Apps (M12–M21)

    [x] Milestone 12: Crash Inspector & Fault Recovery Dialog (#PF / #GP Handler)

        ID: m12_crash_inspector_dialog

        Goal: When an isolated Userspace ELF faults (#PF or #GP), record its app name, faulting RIP, CR2 address, and exit reason in the kernel process table (Syscall 509) and display a Framework modal dialog on the desktop offering a one-click Restart Application button. Emit CRASH_INSPECTOR:PASS:Fault_Capture_Ready.

        Files: src/arch/x86_64/interrupts.rs, src/task/mod.rs, userspace/src/framework/app.rs

        Expect: CRASH_INSPECTOR:PASS:Fault_Capture_Ready

        Steps: []

    [x] Milestone 13: Live UI Inspector & Widget Bounds Overlay (F12 DevTools)

        ID: m13_ui_inspector_devtools

        Goal: Add a Framework-level UI Inspector (F12 toggle or Test app switch) that highlights hovered widget bounds, padding, corner radius, adaptive text luminance score, and per-frame render time in milliseconds. Emit UI_INSPECTOR:PASS:Overlay_Active.

        Files: userspace/src/framework/app.rs, userspace/src/framework/canvas.rs, userspace/src/apps/test_app.rs

        Expect: UI_INSPECTOR:PASS:Overlay_Active

        Steps: []

    [x] Milestone 14: Quick Command Palette (Alt+Space Launcher)

        ID: m14_command_palette

        Goal: Implement a centered frosted-glass Command Palette (Alt+Space) built with Framework TextInputWidget and ScrollViewWidget for instant fuzzy launching of apps, opening files in EOS SHARE, or running system commands. Emit CMD_PALETTE:PASS:AltSpace_Ready.

        Files: userspace/src/framework/menu.rs, userspace/src/framework/app.rs

        Expect: CMD_PALETTE:PASS:AltSpace_Ready

        Steps: []

    [x] Milestone 15: Toast Notification Overlay & Modal Dialogs (Confirm, Rename, NewFolder)

        ID: m15_toasts_and_modal_dialogs

        Goal: Wire PortalKind::Notifications to render smooth slide-in Aero/Modern toast banners in the bottom-right corner, and add reusable ModalDialog (Confirm, TextPrompt) in userspace/src/framework/tools.rs. Emit MODAL_TOAST:PASS:Portals_Wired.

        Files: userspace/src/framework/tools.rs, userspace/src/framework/portal.rs, userspace/src/framework/app.rs

        Expect: MODAL_TOAST:PASS:Portals_Wired

        Steps: []

    [ ] Milestone 15b (Framework UI Polish Backlog): Universal Button Hover States, Wallpaper Thumbnail Clipping, Bilinear Downscaling & Non-Blocking Wallpaper Scan

        ID: m15b_framework_ui_polish

        Goal: Add universal hover states to Framework buttons/controls, enforce strict Canvas clip bounds on wallpaper thumbnails in Settings, upgrade thumbnail/image downscaling from nearest-neighbor to bilinear filtering to eliminate pixelation, and decode large wallpapers asynchronously/chunked without freezing UI.

        Files: userspace/src/framework/controls.rs, userspace/src/apps/settings.rs, userspace/src/framework/canvas.rs

        Expect: FRAMEWORK_POLISH:PASS:Hover_Bilinear_Ready

        Steps: []

    [ ] Milestone 16: Finder Full File Management & Context-Aware Actions

        ID: m16_finder_file_operations

        Goal: Upgrade userspace/src/apps/finder.rs with working SmoothScrollBar drag/click, right-click item-specific context menus (Open, Open in Terminal, Rename, New Folder, Copy Path, Delete), and bottom details bar. Emit FINDER_OPS:PASS:Full_CRUD_Ready.

        Files: userspace/src/apps/finder.rs, src/fs/mod.rs, src/drivers/ata.rs

        Expect: FINDER_OPS:PASS:Full_CRUD_Ready

        Steps: []

    [ ] Milestone 17: Video Player Volume Slider, Subtitle Sync & Folder Playlist

        ID: m17_video_player_pro

        Goal: Add an interactive volume slider + mute toggle (M), subtitle delay offset ([ / ] keys, +/- 100ms), automatic next/previous video playlist in the same directory, and bilinear/SIMD frame scaling in userspace/src/apps/video_player.rs. Emit VIDEO_PRO:PASS:Volume_Subs_Playlist.

        Files: userspace/src/apps/video_player.rs, userspace/src/ffi_ffmpeg.c

        Expect: VIDEO_PRO:PASS:Volume_Subs_Playlist

        Steps: []

    [ ] Milestone 18: Activity Monitor Live Kernel Telemetry & Process Control

        ID: m18_activity_monitor_real_telemetry

        Goal: Replace static mock data in userspace/src/apps/activity_monitor.rs with real sdk::sys::query_processes() (Syscall 509), per-core SMP utilization, kernel heap usage, and a working Terminate Process button. Emit ACTIVITY_MON:PASS:Live_Syscall509.

        Files: userspace/src/apps/activity_monitor.rs, src/task/mod.rs

        Expect: ACTIVITY_MON:PASS:Live_Syscall509

        Steps: []

    [ ] Milestone 19: Terminal Tab Auto-Completion, Output Redirection & System Commands

        ID: m19_terminal_shell_upgrades

        Goal: Add Tab path/command auto-completion, > / >> file redirection, and built-in ps, kill, free, lspci, reboot, and shutdown commands to userspace/src/apps/terminal.rs. Emit TERMINAL_PRO:PASS:Completion_And_SysCmds.

        Files: userspace/src/apps/terminal.rs

        Expect: TERMINAL_PRO:PASS:Completion_And_SysCmds

        Steps: []

    [ ] Milestone 20: TextEdit Syntax Highlighting, Undo/Redo & Save-As Portal

        ID: m20_textedit_code_features

        Goal: Upgrade userspace/src/apps/textedit.rs with keyword/string/comment syntax highlighting for .rs, .py, .c, .ini, and .json, multi-step Ctrl+Z / Ctrl+Y Undo/Redo, and Save As via FileDialog. Emit TEXTEDIT_PRO:PASS:Syntax_Undo_Ready.

        Files: userspace/src/apps/textedit.rs

        Expect: TEXTEDIT_PRO:PASS:Syntax_Undo_Ready

        Steps: []

    [ ] Milestone 21: Global Keyboard Shortcuts & Screenshot Capture (PrintScreen)

        ID: m21_global_shortcuts_screenshot

        Goal: Add Alt+F4 (close active window), Win+D (toggle desktop), Win+Left/Right (Aero Snap), and PrintScreen (encode active framebuffer to PNG via png crate and save to /EOS SHARE/screenshot_<ts>.png with toast notification). Emit SHORTCUTS_SCREENSHOT:PASS:Verified.

        Files: userspace/src/framework/app.rs, userspace/src/png.rs

        Expect: SHORTCUTS_SCREENSHOT:PASS:Verified

        Steps: []

Phase 4: Static CPython 3.12 Runtime & System Automation (M22–M26)

    [ ] Milestone 22: Static libpython3.12.a Cross-Compilation & FFI Bridge (ffi_python.c)

        ID: m22_cpython_static_link

        Goal: Compile CPython 3.12 (--disable-shared --without-ensurepip with musl-gcc) into userspace/vendor/python/lib/libpython3.12.a, link it in userspace/build.rs, and initialize the embedded interpreter in userspace/src/ffi_python.rs. Emit CPYTHON_LINK:PASS:Embedded_3_12_Ready.

        Files: userspace/build.rs, userspace/src/ffi_python.c, userspace/src/ffi_python.rs

        Expect: CPYTHON_LINK:PASS:Embedded_3_12_Ready

        Steps: []

    [ ] Milestone 23: eos Built-in Python C-Extension Module (eos.gui, eos.sys, eos.audio)

        ID: m23_cpython_eos_module

        Goal: Expose native EOS syscalls and portal actions as a built-in CPython module eos (eos.spawn(app), eos.notify(title, msg), eos.processes(), eos.clipboard_get(), eos.clipboard_set(), eos.audit(msg)). Emit CPYTHON_EOS_MOD:PASS:Bindings_Active.

        Files: userspace/src/ffi_python.c, userspace/src/ffi_python.rs

        Expect: CPYTHON_EOS_MOD:PASS:Bindings_Active

        Steps: []

    [ ] Milestone 24: python CLI Execution & Interactive REPL in Terminal & tty2

        ID: m24_terminal_cpython_runner

        Goal: Enable python script.py and python -c "..." in Terminal and tty2 Root Shell to execute real Python 3.12 scripts with stdout/stderr routed directly to the console. Emit CPYTHON_CLI:PASS:Script_Execution_OK.

        Files: userspace/src/apps/terminal.rs, userspace/src/ffi_python.rs

        Expect: CPYTHON_CLI:PASS:Script_Execution_OK

        Steps: []

    [ ] Milestone 25: TEST_SUBJECT/01_math_matrix.py Real CPython Verification

        ID: m25_cpython_test_subject_verify

        Goal: Run /EOS SHARE/TEST_SUBJECT/01_math_matrix.py via embedded CPython during startup verification, assert TRACE_10=285, record in eos_output.json, and emit CPYTHON_VERIFY:PASS:TRACE_10_285.

        Files: userspace/src/test_subject.rs, userspace/src/ffi_python.rs

        Expect: CPYTHON_VERIFY:PASS:TRACE_10_285

        Steps: []

    [ ] Milestone 26: Background Python Automation Service (eos_auto.py)

        ID: m26_cpython_background_service

        Goal: Support running user automation scripts from /RootFS/autostart/ in a dedicated isolated ELF (eos_python.elf --script ...) without blocking UI frame pacing. Emit CPYTHON_AUTO:PASS:Isolated_Worker_Ready.

        Files: userspace/src/main.rs, userspace/src/ffi_python.rs

        Expect: CPYTHON_AUTO:PASS:Isolated_Worker_Ready

        Steps: []

Phase 5: Bare-Metal Hardware Drivers & Real-Machine Readiness (M27–M32)

    [ ] Milestone 27: PAT / Write-Combining (WC) Framebuffer Acceleration

        ID: m27_hw_write_combining_fb

        Goal: Configure x86_64 Page Attribute Table (IA32_PAT MSR 0x277) in src/mm/paging.rs and map UEFI GOP / PCI linear framebuffers as Write-Combining (WC), boosting 2D software/compositor blit throughput to full memory bandwidth on bare-metal laptops and desktops. Emit HW_PAT_WC:PASS:Framebuffer_Accelerated.

        Files: src/mm/paging.rs, src/main.rs

        Expect: HW_PAT_WC:PASS:Framebuffer_Accelerated

        Steps: []

    [ ] Milestone 28: AHCI / SATA DMA Storage Driver (PCI Class 01:06)

        ID: m28_hw_ahci_sata_driver

        Goal: Implement src/drivers/ahci.rs supporting PCI AHCI HBA discovery, port FIS/Command List initialization, and DMA sector read/write, wired into src/fs/mod.rs alongside legacy ATA. Emit HW_AHCI:PASS:SATA_DMA_Ready.

        Files: src/drivers/ahci.rs, src/drivers/mod.rs, src/fs/mod.rs, src/main.rs

        Expect: HW_AHCI:PASS:SATA_DMA_Ready

        Steps: []

    [ ] Milestone 29: Intel High Definition Audio Driver (Intel HDA — PCI Class 04:03)

        ID: m29_hw_intel_hda_audio

        Goal: Implement src/drivers/hda.rs (CORB/RIRB verb discovery, output DAC/Pin widget unmuting, and 48kHz Stereo BDL DMA playback) as automatic hardware backend alongside AC'97 in Syscall 540..542. Emit HW_HDA:PASS:Intel_HDA_Ready.

        Files: src/drivers/hda.rs, src/drivers/mod.rs, src/arch/x86_64/syscall.rs, src/main.rs

        Expect: HW_HDA:PASS:Intel_HDA_Ready

        Steps: []

    [ ] Milestone 30: Realtek RTL8111/8168 Gigabit Ethernet Driver

        ID: m30_hw_rtl8111_ethernet

        Goal: Implement src/drivers/rtl8111.rs (Vendor 0x10EC, Device 0x8168/0x8136) with RX/TX descriptor rings wired into src/net.rs alongside Intel E1000. Emit HW_NIC_RTL:PASS:Ethernet_Ready.

        Files: src/drivers/rtl8111.rs, src/drivers/mod.rs, src/net.rs, src/main.rs

        Expect: HW_NIC_RTL:PASS:Ethernet_Ready

        Steps: []

    [ ] Milestone 31: Clean ACPI Shutdown, Reboot & Disk Sync

        ID: m31_clean_power_management

        Goal: Add Syscall 550 (sys_power(cmd: 1=Reboot, 2=Shutdown)) that flushes VFS buffers and invokes src/arch/x86_64/power.rs, wired to Start Menu and tty2 root commands. Emit POWER_MGR:PASS:Clean_Shutdown_Ready.

        Files: src/arch/x86_64/syscall.rs, src/arch/x86_64/power.rs, userspace/src/framework/menu.rs

        Expect: POWER_MGR:PASS:Clean_Shutdown_Ready

        Steps: []

    [ ] Milestone 32: Full System Stress & Bare-Metal Readiness Certification

        ID: m32_eos_gold_certification

        Goal: Run end-to-end verification across multi-process GUI apps, RTL/Arabic rendering, CPython automation, Ctrl+Alt+F2 TTY switching, and hardware drivers, exporting /EOS SHARE/TEST_SUBJECT/eos_certification.json and emitting EOS_CERTIFIED:PASS:32_OF_32.

        Files: userspace/src/test_subject.rs, test_eos_audit.py

        Expect: EOS_CERTIFIED:PASS:32_OF_32

        Steps: []
