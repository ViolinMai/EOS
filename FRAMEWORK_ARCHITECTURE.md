# EOS Framework & Syscall-First Architecture

## 1. Upstream-First Propagation Rule
All UI primitives, rendering effects, input routing, and hardware acceleration live exclusively inside:
- `userspace/src/framework/` (`Canvas`, `WindowFrame`, `controls.rs`, `theme.rs`, `features.rs`, `animator.rs`)
- `userspace/src/sdk.rs` (`sdk::window`, `sdk::gpu`, `sdk::media`, `sdk::agent`)

**Policy:**
- Applications in `userspace/src/apps/` **must never** re-implement rendering effects, image decoders, or raw hardware syscalls.
- When a feature is upgraded in `framework/` or `sdk.rs`, **every application** (`Finder`, `Settings`, `Terminal`, `TextEdit`, `Browser`, `ActivityMonitor`, `Preview`, `Test`) automatically adopts the upgrade with zero per-app edits.

---

## 2. Beta -> Stable Feature Promotion Workflow
All experimental framework features are gated behind `userspace/src/framework/features.rs` (`FrameworkChannel::Beta` vs `FrameworkChannel::Stable`) and validated inside the **`Test`** reference application (`userspace/src/apps/test_app.rs`):

1. **Prototype in Upstream (`Beta`):** Implement the new feature in `framework/` behind a feature flag in `features.rs`.
2. **Verify in `Test` App:** Open `Test` -> `4. Debug & Beta`, toggle the feature live, and inspect performance (`Avg Frame us`, `Damage Rects Overlay`, `Kernel Heap`).
3. **Promote to `Stable`:** Once verified with zero regressions or `#GP` faults, enable the flag by default in `FrameworkChannel::Stable`. All apps inherit it immediately.

---

## 3. Unified Kernel Syscall Table (Single Source of Truth)
Fixing or accelerating a kernel syscall immediately upgrades every userspace consumer:

| Syscall | Name | Purpose |
| :--- | :--- | :--- |
| `502` | `sys_present` | Full-frame atomic presentation (VirtIO-GPU or Software fallback) |
| `503` | `sys_poll_event` | Non-blocking input event poll |
| `504` | `sys_present_rects` | Damage-region atomic presentation (`Buffer Age = 2`) |
| `505` | `sys_wait_event` | Blocking input wait with timeout |
| `506` | `sys_gpu_move_cursor` | Hardware cursor plane movement (`VirtIO-GPU cursorq`) |
| `507` | `sys_gpu_submit_3d` | Submit VirGL 3D command stream to Host OpenGL GPU |
| `508` | `sys_gpu_info` | Query active GPU backend, resolution, VirGL 3D, and HW cursor state |
| `510` | `sys_media_decode_image` | Centralized kernel image decoder (PNG/JPEG) via `sdk::media::decode_image` |
| `520` | `sys_agent_dispatch` | Telemetry, audit markers, and heap diagnostics |

---

## 4. EOS Portal System (XDG-Style Service Bus)
Implemented in `userspace/src/framework/portal.rs`. Apps never talk directly to each other's internals; they request system capabilities through the Framework Portal Bus:
- **Exclusive Portals (`PortalAccess::Exclusive`):** Assigned to a single authoritative provider (e.g., `FilePicker` -> `Finder`, `Settings` -> `Settings`, `Screenshot` -> `EOS Shell`). Unauthorized provider hijacking is rejected.
- **Unlimited Portals (`PortalAccess::Unlimited`):** Open to any number of consumers/providers (e.g., `AudioOutput`, `Notifications`, `Clipboard`).

## 5. Hardware vs Software Render Toggle
Software rendering is preserved intact and never deleted. To switch modes, edit `src/config.rs`:
- `hw_accel_3d`: `true` (VirGL 3D) / `false` (2D/Software)
- `hw_cursor`: `true` (VirtIO Hardware Cursor) / `false` (Software Cursor)
