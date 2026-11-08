//! Read-only information snapshot boundary; callers cannot select paths/commands.
use mlua::{Lua, Table};
use tohru_shell::info::{IssueKind, Snapshot};

fn encode(lua: &Lua, snapshot: Snapshot) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    let system = lua.create_table()?;
    let value = snapshot.system;
    system.set("os_name", value.os_name)?;
    system.set("os_version", value.os_version)?;
    system.set("hostname", value.hostname)?;
    system.set("kernel", value.kernel)?;
    system.set("processor", value.processor)?;
    system.set("graphics", lua.create_sequence_from(value.graphics)?)?;
    system.set("memory_total_bytes", value.memory_total_bytes)?;
    system.set("memory_available_bytes", value.memory_available_bytes)?;
    system.set("uptime_seconds", value.uptime_seconds)?;
    system.set("current_uid", value.current_uid)?;
    system.set("current_generation", value.current_generation)?;
    system.set("configuration_revision", value.configuration_revision)?;
    system.set("system_target", value.system_target)?;
    result.set("system", system)?;

    let accounts = lua.create_table()?;
    for (index, account) in snapshot.accounts.into_iter().enumerate() {
        let row = lua.create_table()?;
        row.set("name", account.name)?;
        row.set("uid", account.uid)?;
        row.set("description", account.description)?;
        row.set("home", account.home)?;
        row.set("shell", account.shell)?;
        accounts.raw_set(index + 1, row)?;
    }
    result.set("accounts", accounts)?;
    let disks = lua.create_table()?;
    for (index, disk) in snapshot.disks.into_iter().enumerate() {
        let row = lua.create_table()?;
        row.set("name", disk.name)?;
        row.set("model", disk.model)?;
        row.set("capacity_bytes", disk.capacity_bytes)?;
        row.set("removable", disk.removable)?;
        disks.raw_set(index + 1, row)?;
    }
    result.set("disks", disks)?;
    let mounts = lua.create_table()?;
    for (index, mount) in snapshot.mounts.into_iter().enumerate() {
        let row = lua.create_table()?;
        row.set("path", mount.path)?;
        row.set("filesystem", mount.filesystem)?;
        row.set("source", mount.source)?;
        row.set("read_only", mount.read_only)?;
        mounts.raw_set(index + 1, row)?;
    }
    result.set("mounts", mounts)?;
    let generations = lua.create_table()?;
    for (index, generation) in snapshot.generations.into_iter().enumerate() {
        let row = lua.create_table()?;
        row.set("number", generation.number)?;
        row.set("target", generation.target)?;
        row.set("current", generation.current)?;
        generations.raw_set(index + 1, row)?;
    }
    result.set("generations", generations)?;
    let issues = lua.create_table()?;
    for (index, issue) in snapshot.issues.into_iter().enumerate() {
        let row = lua.create_table()?;
        row.set("source", issue.source)?;
        row.set(
            "kind",
            match issue.kind {
                IssueKind::Io(kind) => format!("{kind:?}"),
                IssueKind::TooLarge => "TooLarge".to_owned(),
                IssueKind::Invalid => "Invalid".to_owned(),
                IssueKind::Budget => "Budget".to_owned(),
            },
        )?;
        issues.raw_set(index + 1, row)?;
    }
    result.set("issues", issues)?;
    Ok(result)
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let api = lua.create_table()?;
    api.set(
        "snapshot",
        lua.create_function(|lua, args: mlua::MultiValue| {
            if !args.is_empty() {
                return Err(mlua::Error::runtime(
                    "information snapshot accepts no arguments",
                ));
            }
            encode(lua, tohru_shell::info::snapshot())
        })?,
    )?;
    lua.globals().set("tohru_info", api)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tohru_shell::info::{Account, Disk, Generation, Issue};

    #[test]
    fn missing_zero_false_and_generation_identity_remain_distinct() {
        let lua = Lua::new();
        install(&lua).unwrap();
        lua.load("assert(type(tohru_info.snapshot)=='function'); assert(not pcall(tohru_info.snapshot,'/etc/shadow'))").exec().unwrap();
        lua.load("local s=tohru_info.snapshot(); assert(type(s.system)=='table' and type(s.accounts)=='table' and type(s.disks)=='table' and type(s.mounts)=='table' and type(s.generations)=='table' and #s.issues<=32)").exec().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.system.current_uid = Some(0);
        snapshot.system.memory_available_bytes = Some(0);
        snapshot.accounts.push(Account {
            name: "root".into(),
            uid: 0,
            description: String::new(),
            home: "/root".into(),
            shell: "/bin/sh".into(),
        });
        snapshot.disks.push(Disk {
            name: "disk".into(),
            model: None,
            capacity_bytes: None,
            removable: Some(false),
        });
        snapshot.generations.push(Generation {
            number: 42,
            target: "/nix/store/selected".into(),
            current: false,
        });
        snapshot.issues.push(Issue {
            source: "/bounded/public".into(),
            kind: IssueKind::Io(std::io::ErrorKind::PermissionDenied),
        });
        lua.globals()
            .set("result", encode(&lua, snapshot).unwrap())
            .unwrap();
        lua.load("assert(result.system.os_name==nil and result.system.current_generation==nil); assert(result.system.current_uid==0 and result.system.memory_available_bytes==0); assert(result.accounts[1].uid==0 and result.accounts[1].home=='/root'); assert(result.disks[1].model==nil and result.disks[1].capacity_bytes==nil and result.disks[1].removable==false); assert(result.generations[1].number==42 and result.generations[1].current==false); assert(result.issues[1].kind=='PermissionDenied' and #result.mounts==0 and #result.system.graphics==0)").exec().unwrap();
    }
}
