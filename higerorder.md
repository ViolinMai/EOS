# EOS Higher-Order Escalation Log (`higerorder.md`)

This file is automatically populated by `agent_runner.py` whenever the autonomous Userspace Agent encounters an architectural blocker that requires changes **outside** `userspace/` (such as Kernel syscalls in `src/arch/x86_64/syscall.rs`, VFS drivers in `src/fs/`, hardware drivers in `src/drivers/`, or QEMU configuration in `run_qemu.py`).

**Rule:** The autonomous agent is strictly forbidden from modifying any file outside `userspace/`. All kernel-level or host-level requests are logged below for human-architect review.

---

## Pending Higher-Order Requests

*(No kernel/host blockers logged yet. The agent will append structured entries here if blocked.)*
