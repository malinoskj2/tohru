# Native lock authority and secret export boundary

User-authorized narrow extension of pinned Pleamar. This is source/checkpoint
work awaiting native compilation and independent security acceptance; do not
activate production lock roles from it yet. Native PAM and session-lock backend
remain authoritative. Generic `auth.check` remains a pure authentication query.

Native API:
- Root Luau `sys.ask("auth.unlock", password)` snapshots an actual confirmed
  native lock cycle BEFORE PAM, checks native PAM authentication and account
  policy, and sends an opaque nonclone cycle-bound Permit to the renderer.
  Root/plugin ownership is captured from immutable Rust `LuauScript.prefix`,
  never a caller name, alias, fact or user argument. Platform query cannot mint.
- `sys.ask("lock.held")` is an immutable read of actual native protocol state.
  The locker rechecks this before suspend and authentication; writable mirrors
  cannot qualify as readiness. External/root fact writes to lock.held are denied.
- Only an internal AuthenticatedUnlock message can reserve release. The final
  native Wayland function validates cycle identity+owner+held state, reserves
  once, drops the engaged mutex for GPU teardown, then revalidates the SAME
  reservation before the sole native unlock call. No mutex is held over PAM,
  GPU destruction or event-loop dispatch. Rejected permits change no faces.
- Native Finished revokes the cycle and permits disposal without an unlock.
  New acquisition is blocked until a same-queue wl_display.sync retirement
  barrier drains terminal callbacks from the old cycle. No timer or busy loop.
  On disconnect, compositor lock remains the authority and no unlock is issued.
- Public open=false expressions/facts/events cannot remove active lock faces.
  Scene-definition reload is rejected from request/acquisition until native
  release/Finished; logic VM reload remains available and carries no capability.
  After authenticated release, suppress reacquisition until requested open has
  been observed false, so asynchronous UI publication cannot instantly relock.

Secret metadata uses native compiler-retained SECRETS names, including renamed/
cloned fields compiled by native language. `get`, `wait` predicates/diagnostics
and --record redact or reject before accessing text. Public pending waits are
cancelled on definition reload so text indices cannot rebind to a secret; current
metadata is rechecked before queued condition evaluation/diagnostics. Existing
watch/describe/action diffs use native masking and remain separate regression gates.
No new generic service framework, daemon, dependency or PAM implementation.

S-owned shared wiring request:
1. Apply `runtime/patches/0002-lock-boundary.patch` after existing settings patch.
2. Copy `runtime/lock/native.rs` to upstream `src/tohru_lock.rs` and
   `runtime/lock/authority.rs` to `src/tohru_lock/authority.rs`.
3. Copy `tests/session/lock_authority.rs`, `native_security.rs`, `native_binding.rs`
   and `security-*` fixtures to upstream `tests/session/` for focused native tests.
4. Include these owned files in Nix sources/prepare; no new dependencies.

CPU core: `rustc --edition 2024 --test runtime/lock/authority.rs -o .tmp/lock-authority-test`
then `.tmp/lock-authority-test` (6 tests). Feature script:
`lua tests/session/logic.lua`. Patched native focused tests:
`cargo test --release --locked --lib tohru_lock:: --jobs 1` and
`cargo test --release --locked --lib auth_unlock_uses_immutable_root --jobs 1`.
Heavy build and GPU/private native checks require coordinator allocations.
Successful controlled native PAM, secret native read/export checks and stale/
retirement/reload/native timing coverage remain required before safety acceptance.
