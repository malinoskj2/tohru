use super::read::{Reader, bounded};
use super::{Disk, Generation, IssueKind, Mount};

pub(super) fn disks(reader: &mut Reader<'_>) -> Vec<Disk> {
    let mut out = Vec::new();
    for name in reader.names("sys/class/block", 128) {
        if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("zram") {
            continue;
        }
        if out.len() == 64 {
            reader.issue("sys/class/block", IssueKind::TooLarge);
            break;
        }
        let prefix = format!("sys/class/block/{name}");
        let capacity_bytes = reader
            .line(&format!("{prefix}/size"))
            .and_then(|text| text.parse::<u64>().ok())
            .and_then(|sectors| sectors.checked_mul(512));
        let removable = reader
            .line(&format!("{prefix}/removable"))
            .and_then(|text| match text.as_str() {
                "0" => Some(false),
                "1" => Some(true),
                _ => None,
            });
        let model = reader.line(&format!("{prefix}/device/model"));
        out.push(Disk {
            name,
            model,
            capacity_bytes,
            removable,
        });
    }
    out
}

fn mount_word(word: &str) -> Option<String> {
    let mut out = Vec::new();
    let mut bytes = word.bytes();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        let digits = [bytes.next()?, bytes.next()?, bytes.next()?];
        if !digits.iter().all(|digit| (b'0'..=b'7').contains(digit)) {
            return None;
        }
        let value = u16::from(digits[0] - b'0') * 64
            + u16::from(digits[1] - b'0') * 8
            + u16::from(digits[2] - b'0');
        out.push(u8::try_from(value).ok()?);
    }
    String::from_utf8(out).ok().map(|text| bounded(&text))
}

pub(super) fn mounts(reader: &mut Reader<'_>) -> Vec<Mount> {
    let Some(text) = reader.text("proc/self/mountinfo", 128 << 10) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if index >= 128 {
            reader.issue("proc/self/mountinfo", IssueKind::TooLarge);
            break;
        }
        let parsed = (|| {
            let (mount, filesystem) = line.split_once(" - ")?;
            let mut mount = mount.split_whitespace();
            let path = mount_word(mount.nth(4)?)?;
            let options = mount.next()?;
            let mut filesystem = filesystem.split_whitespace();
            let kind = bounded(filesystem.next()?);
            let source = mount_word(filesystem.next()?)?;
            Some(Mount {
                path,
                filesystem: kind,
                source,
                read_only: options.split(',').any(|option| option == "ro"),
            })
        })();
        if let Some(mount) = parsed {
            out.push(mount);
        } else {
            reader.issue("proc/self/mountinfo", IssueKind::Invalid);
        }
    }
    out
}

pub(super) fn generation_number(name: &str) -> Option<u64> {
    let number = name.strip_prefix("system-")?.strip_suffix("-link")?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

pub(super) fn generations(reader: &mut Reader<'_>, current: Option<&str>) -> Vec<Generation> {
    let mut out = Vec::new();
    let failures = reader.failure_count();
    for name in reader.names("nix/var/nix/profiles", 256) {
        let Some(number) = generation_number(&name) else {
            continue;
        };
        let source = format!("nix/var/nix/profiles/{name}");
        if let Some(target) = reader.link(&source) {
            let identity = current.and_then(|_| reader.system_identity(&source));
            if current.is_some() && identity.is_none() {
                reader.issue(&source, IssueKind::Invalid);
            }
            let active = current.is_some() && identity.as_deref() == current;
            out.push(Generation {
                number,
                target,
                current: active,
            });
        }
    }
    if reader.failure_count() != failures
        || out.iter().filter(|generation| generation.current).count() > 1
    {
        // A partial scan or unresolved eligible target cannot establish a unique
        // active generation. Retain partial rows, but label no alias current.
        for generation in &mut out {
            generation.current = false;
        }
        reader.issue("run/current-system", IssueKind::Invalid);
    }
    out.sort_by_key(|generation| std::cmp::Reverse(generation.number));
    out
}
