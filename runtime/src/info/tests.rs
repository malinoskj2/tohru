use super::*;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tohru-info-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn file(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[cfg(unix)]
    fn link(&self, path: &str, target: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, path).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn fixed_sources_produce_actual_typed_facts_and_public_accounts() {
    let fixture = Fixture::new();
    fixture.file(
        "etc/os-release",
        "NAME=\"Fixture Linux\"\nVERSION='42 (Harbor)'\n",
    );
    fixture.file("proc/sys/kernel/hostname", "fixture-host\n");
    fixture.file("proc/sys/kernel/osrelease", "6.12.0-test\n");
    fixture.file("proc/cpuinfo", "processor : 0\nmodel name\t: Fixture CPU\n");
    fixture.file("proc/meminfo", "MemTotal: 4096 kB\nMemAvailable: 1024 kB\n");
    fixture.file("proc/uptime", "1234.98 8888.00\n");
    fixture.file(
        "proc/self/status",
        "Name:\ttest\nUid:\t1000\t1000\t1000\t1000\n",
    );
    fixture.file(
        "etc/passwd",
        "root:x:0:0:Root:/root:/bin/sh\nalice:x:1000:100:Alice:/home/alice:/bin/zsh\n",
    );
    fixture.file(
        "proc/driver/nvidia/gpus/0000:01:00.0/information",
        "Model: Fixture GPU\n",
    );
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.os_name.as_deref(), Some("Fixture Linux"));
    assert_eq!(snapshot.system.os_version.as_deref(), Some("42 (Harbor)"));
    assert_eq!(snapshot.system.processor.as_deref(), Some("Fixture CPU"));
    assert_eq!(snapshot.system.memory_total_bytes, Some(4_194_304));
    assert_eq!(snapshot.system.memory_available_bytes, Some(1_048_576));
    assert_eq!(snapshot.system.uptime_seconds, Some(1234));
    assert_eq!(snapshot.system.current_uid, Some(1000));
    assert_eq!(snapshot.system.graphics, vec!["Fixture GPU"]);
    assert_eq!(snapshot.accounts[1].name, "alice");
    assert_eq!(snapshot.accounts[1].description, "Alice");
}

#[test]
fn missing_invalid_and_overflow_are_unknown_never_fabricated_zero() {
    let fixture = Fixture::new();
    fixture.file(
        "proc/meminfo",
        "MemTotal: 18446744073709551615 kB\nMemAvailable: nonsense kB\n",
    );
    fixture.file(
        "etc/passwd",
        "invalid\nname:x:bad:0:Name:/home/name:/bin/sh\n",
    );
    fixture.file("sys/class/block/sda/size", "18446744073709551615\n");
    let snapshot = collect(&fixture.0);
    assert!(snapshot.system.os_name.is_none() && snapshot.system.hostname.is_none());
    assert!(
        snapshot.system.memory_total_bytes.is_none()
            && snapshot.system.memory_available_bytes.is_none()
    );
    assert!(snapshot.accounts.is_empty());
    assert_eq!(snapshot.disks.len(), 1);
    assert_eq!(snapshot.disks[0].capacity_bytes, None);
    assert!(
        snapshot
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Invalid)
    );
    assert!(snapshot.issues.len() <= 32);
}

#[cfg(unix)]
#[test]
fn mounts_capacities_and_generations_are_read_only_metadata() {
    let fixture = Fixture::new();
    fixture.file("sys/class/block/nvme0n1p2/size", "2048\n");
    fixture.file("sys/class/block/nvme0n1p2/removable", "0\n");
    fixture.file("sys/class/block/nvme0n1p2/device/model", "Fixture Disk\n");
    fixture.file("proc/self/mountinfo","35 1 8:2 / / rw,relatime - ext4 /dev/nvme0n1p2 rw\n36 1 8:3 / /media/a\\040b ro - ext4 /dev/sda3 ro\nmalformed\n");
    fixture.file(
        "nix/store/fixture-system42/configuration-revision",
        "abcdef\n",
    );
    fixture.link("run/current-system", "../nix/store/fixture-system42");
    fixture.link("nix/var/nix/profiles/system", "system-42-link");
    fixture.link(
        "nix/var/nix/profiles/system-42-link",
        "/nix/store/fixture-system42",
    );
    fixture.file("nix/store/fixture-system2/configuration-revision", "old\n");
    fixture.link(
        "nix/var/nix/profiles/system-2-link",
        "/nix/store/fixture-system2",
    );
    fixture.link(
        "nix/var/nix/profiles/system-invalid-link",
        "/nix/store/never-read",
    );
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.disks[0].capacity_bytes, Some(1_048_576));
    assert_eq!(snapshot.disks[0].removable, Some(false));
    assert_eq!(snapshot.mounts[1].path, "/media/a b");
    assert!(snapshot.mounts[1].read_only);
    assert_eq!(
        snapshot.system.configuration_revision.as_deref(),
        Some("abcdef")
    );
    assert_eq!(snapshot.system.current_generation, Some(42));
    assert_eq!(
        snapshot
            .generations
            .iter()
            .map(|g| g.number)
            .collect::<Vec<_>>(),
        vec![42, 2]
    );
    assert!(snapshot.generations[0].current && !snapshot.generations[1].current);
    assert_eq!(
        fs::read_link(fixture.0.join("nix/var/nix/profiles/system")).unwrap(),
        PathBuf::from("system-42-link")
    );
}

#[test]
fn file_directory_record_and_cumulative_read_bounds_are_real() {
    let fixture = Fixture::new();
    fixture.file("etc/os-release", &"X".repeat(16385));
    fixture.file(
        "etc/passwd",
        &(0..300).fold(String::new(), |mut text, index| {
            writeln!(text, "user{index}:x:{index}:0:User:/home/user:/bin/sh").unwrap();
            text
        }),
    );
    let snapshot = collect(&fixture.0);
    assert!(snapshot.system.os_name.is_none());
    assert_eq!(snapshot.accounts.len(), 256);
    assert!(
        snapshot
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::TooLarge)
    );
    for index in 0..5 {
        fixture.file(&format!("large{index}"), &"x".repeat(300_000));
    }
    let mut reader = read::Reader::new(&fixture.0);
    assert!(reader.text("large0", 300_000).is_some());
    assert!(reader.text("large1", 300_000).is_some());
    assert!(reader.text("large2", 300_000).is_some());
    assert!(reader.text("large3", 300_000).is_none());
    assert!(reader.text("large4", 300_000).is_none());
    for index in 0..10 {
        fixture.file(&format!("entries/{index}"), "");
    }
    let mut reader = read::Reader::new(&fixture.0);
    assert_eq!(reader.names("entries", 3).len(), 3);
    assert!(
        reader
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::TooLarge)
    );
}

#[test]
fn no_shell_evaluation_or_unbounded_public_field_strings() {
    let fixture = Fixture::new();
    fixture.file(
        "etc/os-release",
        "NAME=\"$(touch never)\"\nVERSION='${HOME}'\n",
    );
    fixture.file("proc/sys/kernel/hostname", &"界".repeat(300));
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.os_name.as_deref(), Some("$(touch never)"));
    assert_eq!(snapshot.system.os_version.as_deref(), Some("${HOME}"));
    assert_eq!(snapshot.system.hostname.unwrap().chars().count(), 256);
    assert!(!fixture.0.join("never").exists());
}

#[cfg(unix)]
#[test]
fn active_system_generation_is_not_selected_profile_and_unknown_stays_unknown() {
    let fixture = Fixture::new();
    fixture.file("nix/store/system41/configuration-revision", "old\n");
    fixture.file("nix/store/system42/configuration-revision", "new\n");
    fixture.link("run/current-system", "../nix/store/system41");
    fixture.link("nix/var/nix/profiles/system", "system-42-link");
    fixture.link("nix/var/nix/profiles/system-41-link", "/nix/store/system41");
    fixture.link(
        "nix/var/nix/profiles/system-42-link",
        "../../../store/system42",
    );
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.current_generation, Some(41));
    assert!(!snapshot.generations[0].current && snapshot.generations[1].current);
    fs::remove_file(fixture.0.join("run/current-system")).unwrap();
    fixture.link("run/current-system", "../nix/store/system42");
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.current_generation, Some(42));
    assert!(snapshot.generations[0].current && !snapshot.generations[1].current);
    fs::remove_file(fixture.0.join("run/current-system")).unwrap();
    fixture.link("run/current-system", "../nix/store/missing-system");
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.current_generation, None);
    assert!(
        snapshot
            .generations
            .iter()
            .all(|generation| !generation.current)
    );
}

#[cfg(unix)]
#[test]
fn generation_alias_ambiguity_and_symlink_cycles_are_unknown() {
    let fixture = Fixture::new();
    fixture.file("nix/store/system42/configuration-revision", "rev\n");
    fixture.link("run/current-system", "../nix/store/system42");
    fixture.link("nix/var/nix/profiles/system-41-link", "/nix/store/system42");
    fixture.link("nix/var/nix/profiles/system-42-link", "/nix/store/system42");
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.current_generation, None);
    assert!(
        snapshot
            .generations
            .iter()
            .all(|generation| !generation.current)
    );
    fs::remove_file(fixture.0.join("run/current-system")).unwrap();
    fixture.link("run/current-system", "../nix/var/nix/profiles/cycle");
    fixture.link("nix/var/nix/profiles/cycle", "cycle");
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.system_target, None);
    assert!(
        snapshot
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Budget)
    );
}

#[cfg(unix)]
#[test]
fn partial_generation_scans_never_prove_unique_current_alias() {
    let fixture = Fixture::new();
    fixture.file("nix/store/active/configuration-revision", "rev\n");
    fixture.link("run/current-system", "../nix/store/active");
    for number in 0..151 {
        let target = if number == 0 || number == 150 {
            "active".to_owned()
        } else {
            format!("other{number}")
        };
        fixture.file(
            &format!("nix/store/{target}/configuration-revision"),
            "rev\n",
        );
        fixture.link(
            &format!("nix/var/nix/profiles/system-{number:03}-link"),
            &format!("/nix/store/{target}"),
        );
    }
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.current_generation, None);
    assert!(
        snapshot
            .generations
            .iter()
            .all(|generation| !generation.current)
    );
    for number in 151..258 {
        fixture.link(
            &format!("nix/var/nix/profiles/system-{number:03}-link"),
            "/nix/store/active",
        );
    }
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.current_generation, None);
    assert!(
        snapshot
            .generations
            .iter()
            .all(|generation| !generation.current)
    );
}

#[cfg(unix)]
#[test]
fn unresolved_eligible_generation_invalidates_apparent_unique_match() {
    let fixture = Fixture::new();
    fixture.file("nix/store/active/configuration-revision", "rev\n");
    fixture.link("run/current-system", "../nix/store/active");
    fixture.link("nix/var/nix/profiles/system-1-link", "/nix/store/active");
    fixture.link("nix/var/nix/profiles/system-2-link", "/nix/store/missing");
    let snapshot = collect(&fixture.0);
    assert_eq!(snapshot.system.current_generation, None);
    assert!(
        snapshot
            .generations
            .iter()
            .all(|generation| !generation.current)
    );
}
