# Public read-only information snapshots

Existing-package integration only: S adds the module to the existing library,
copies runtime/src in preparation, and exposes `tohru_info.snapshot()` with no
arguments. No new crate/dependency, IPC, service backend, helper or commands.
The typed Rust API is `snapshot() -> Snapshot`; Luau table keys match Rust fields.
Options become nil, u64 observations remain exact in Rust (Luau numeric precision
limits apply above2^53), arrays retain order, issues map {source,kind} with a
readable native error-kind label. Do not turn missing values into0 or empty facts.

Snapshot fields: system.{os_name,os_version,hostname,kernel,processor,graphics,
memory_total_bytes,memory_available_bytes,uptime_seconds,current_uid,
current_generation,configuration_revision,system_target}; accounts{name,uid,
description,home,shell}; disks{name,model,capacity_bytes,removable}; mounts{path,
filesystem,source,read_only}; generations{number,target,current}; issues{source,
kind}. Only pages/About open or explicit refresh call it. Rendering reads cached
presentation; there is no recurring timer, thread or automatic refresh.

Fixed public sources: /etc/os-release and /etc/passwd (never shadow); hostname,
kernel, cpuinfo, meminfo, uptime, self/status, mountinfo under /proc; NVIDIA public
GPU information or DRM vendor/device IDs; block size/removable/model under /sys;
current system public configuration revision and profile generation symlink
names/targets. Block capacity uses512-byte kernel sectors; it is not filesystem
free space. Unsupported mount free/used, avatars, configuration timestamps and
other missing facts remain Unknown; no invented startup disk fallback or admin
links. Generation symlinks are observed, never switched or recursively scanned. Active
generation is derived by matching the resolved /run/current-system identity to
bounded profile generation targets, never by treating the selected system profile
as active. Eight symlink hops under Nix store/profile paths, existing target
directory required; dangling/cyclic/ambiguous aliases leave active generation
Unknown. Generation discovery/resolution must be complete within bounds before
uniqueness is claimed; any partial scan/budget/eligible failure clears all current
flags, even if an earlier row matched. Invalid/non-UTF8/too-long link identities are never truncated for matching.

Limits: one snapshot has1MiB file-byte budget,384 filesystem requests,250ms
cooperative deadline checked between operations; each text4KiB except bounded
os-release/status/meminfo/GPU16KiB, passwd/mountinfo128KiB, cpuinfo512KiB.
Directories are capped8GPU/32DRM/128block/256profile entries; accounts256,
mounts128, disks64; strings256 codepoints, issues32. Byte/entry/read counts are
bounded, but individual regular-file/kernel operations are not a hard250ms
latency guarantee. Linux nonblocking open plus regular-file validation avoids
blocking on substituted FIFOs/devices; no unsafe/FFI or arbitrary UI paths.

Checks (lightweight, no runtime build/GPU; Cargo entry verifies nested module layout):

    cargo test --locked --offline --manifest-path runtime/Cargo.toml info --jobs 1
    cargo clippy --locked --offline --manifest-path runtime/Cargo.toml --all-targets --jobs 1 -- -Dwarnings

    rustfmt --check --edition 2024 runtime/src/info/*.rs runtime/tests/info.rs

IssueKind uses the existing package thiserror derive for typed adapter diagnostics;
no new dependency/manifest edit. Verify through Cargo, which supplies the existing
macro dependency and real nested module context; bare rustc needs that extern.

Ten actual filesystem fixtures cover partial/missing/invalid/overflow facts,
real public record formats and leading names, capacities/escaped mounts/current
generation ordering, unchanged symlink after snapshot, per-file/entry/record and
cumulative bounds, no shell evaluation, UTF-8 field bounds. Rust1.85 test passes.
No source/reference tree edits, live service activation or administrative action.
