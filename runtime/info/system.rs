use super::read::{Reader, bounded};
use super::{Account, IssueKind, System};

fn field<'a>(text: &'a str, key: &str, delimiter: char) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(delimiter)?;
        (name.trim() == key).then_some(value.trim())
    })
}

fn release(text: &str, key: &str) -> Option<String> {
    let value = field(text, key, '=')?;
    // os-release data, never shell evaluation, interpolation or commands.
    let value = if value.starts_with('"') || value.starts_with('\'') {
        let quote = value.chars().next()?;
        value.strip_prefix(quote)?.strip_suffix(quote)?
    } else {
        value
    };
    (!value.is_empty()).then(|| bounded(value))
}

fn kib(text: &str, key: &str) -> Option<u64> {
    let mut fields = field(text, key, ':')?.split_whitespace();
    let value = fields.next()?.parse::<u64>().ok()?;
    (fields.next()? == "kB")
        .then(|| value.checked_mul(1024))
        .flatten()
}

pub(super) fn read(reader: &mut Reader<'_>) -> System {
    let mut out = System::default();
    if let Some(text) = reader.text("etc/os-release", 16384) {
        out.os_name = release(&text, "NAME");
        out.os_version = release(&text, "VERSION").or_else(|| release(&text, "VERSION_ID"));
    }
    out.hostname = reader.line("proc/sys/kernel/hostname");
    out.kernel = reader.line("proc/sys/kernel/osrelease");
    if let Some(text) = reader.text("proc/cpuinfo", 512 << 10) {
        out.processor = field(&text, "model name", ':')
            .or_else(|| field(&text, "Hardware", ':'))
            .map(bounded);
    }
    if let Some(text) = reader.text("proc/meminfo", 16384) {
        out.memory_total_bytes = kib(&text, "MemTotal");
        out.memory_available_bytes = kib(&text, "MemAvailable");
    }
    if let Some(text) = reader.text("proc/uptime", 1024) {
        out.uptime_seconds = text
            .split_whitespace()
            .next()
            .and_then(|value| value.split('.').next())
            .and_then(|value| value.parse().ok());
    }
    if let Some(text) = reader.text("proc/self/status", 16384) {
        out.current_uid = field(&text, "Uid", ':')
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse().ok());
    }
    out.configuration_revision = reader.line("run/current-system/configuration-revision");
    out.system_target = reader.link("run/current-system");
    out.current_generation = reader
        .link("nix/var/nix/profiles/system")
        .and_then(|target| {
            let name = target.rsplit('/').next()?;
            super::storage::generation_number(name)
        });
    // Public NVIDIA product names when available; DRM device IDs remain honest
    // identifiers if the kernel has no human-readable product name.
    for name in reader.names("proc/driver/nvidia/gpus", 8) {
        let text = reader.text(
            &format!("proc/driver/nvidia/gpus/{name}/information"),
            16384,
        );
        if let Some(model) = text.as_deref().and_then(|text| field(text, "Model", ':')) {
            out.graphics.push(bounded(model));
        }
    }
    if out.graphics.is_empty() {
        for name in reader.names("sys/class/drm", 32) {
            if !name
                .strip_prefix("card")
                .is_some_and(|tail| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()))
            {
                continue;
            }
            let vendor = reader.line(&format!("sys/class/drm/{name}/device/vendor"));
            let device = reader.line(&format!("sys/class/drm/{name}/device/device"));
            if let (Some(vendor), Some(device)) = (vendor, device) {
                out.graphics.push(format!("{name}: {vendor}:{device}"));
            }
        }
    }
    out
}

pub(super) fn accounts(reader: &mut Reader<'_>) -> Vec<Account> {
    let Some(text) = reader.text("etc/passwd", 128 << 10) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if index >= 256 {
            reader.issue("etc/passwd", IssueKind::TooLarge);
            break;
        }
        let fields: Vec<_> = line.splitn(8, ':').collect();
        let [name, _, uid, _gid, description, home, shell] = fields.as_slice() else {
            reader.issue("etc/passwd", IssueKind::Invalid);
            continue;
        };
        let Ok(uid) = uid.parse::<u32>() else {
            reader.issue("etc/passwd", IssueKind::Invalid);
            continue;
        };
        out.push(Account {
            name: bounded(name),
            uid,
            description: bounded(description),
            home: bounded(home),
            shell: bounded(shell),
        });
    }
    out
}
