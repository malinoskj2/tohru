//! Narrow shell binding; process state survives replacement of the Luau VM.
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result};
use mlua::{Lua, Table, Value as LuaValue};
use tohru_shell::settings::{Key, Origin, Owner, SetOutcome, Value};

static SETTINGS: OnceLock<Mutex<Owner>> = OnceLock::new();

fn owner() -> Result<&'static Mutex<Owner>> {
    if let Some(owner) = SETTINGS.get() {
        return Ok(owner);
    }
    let user_dir = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => {
            PathBuf::from(std::env::var_os("HOME").context("HOME or XDG_CONFIG_HOME must be set")?)
                .join(".config")
        }
    };
    let managed = std::env::var_os("TOHRU_MANAGED_CONFIG").map(PathBuf::from);
    let owner =
        Owner::load(user_dir.join("tohru/config.toml"), managed).context("load tohru settings")?;
    let _ = SETTINGS.set(Mutex::new(owner));
    SETTINGS.get().context("initialize process settings owner")
}

fn snapshot(lua: &Lua, owner: &Owner) -> mlua::Result<Table> {
    let settings = lua.create_table()?;
    for key in Key::ALL {
        let setting = owner.get(key);
        let row = lua.create_table()?;
        match setting.value {
            Value::Bool(value) => row.set("value", value)?,
            Value::Text(value) => row.set("value", value)?,
            Value::Integer(value) => row.set("value", value)?,
        }
        row.set(
            "origin",
            match setting.origin {
                Origin::Default => "default",
                Origin::User => "user",
                Origin::Nix => "nix",
                Origin::Runtime => "runtime",
            },
        )?;
        row.set("managed", setting.managed)?;
        row.set("runtime_override", setting.runtime_override)?;
        settings.set(key.name(), row)?;
    }
    Ok(settings)
}

fn report(error: impl Into<anyhow::Error>) -> mlua::Error {
    mlua::Error::runtime(format!("{:#}", error.into()))
}

fn applied(lua: &Lua, owner: &Owner, outcome: SetOutcome) -> mlua::Result<(Table, Option<String>)> {
    let warning = match outcome {
        SetOutcome::RuntimeOverride | SetOutcome::Persisted => None,
        SetOutcome::CommittedDurabilityUnconfirmed(warning) => {
            Some(format!("{:#}", anyhow::Error::new(warning)))
        }
    };
    Ok((snapshot(lua, owner)?, warning))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let owner = owner().map_err(report)?;
    let api = lua.create_table()?;
    api.set(
        "snapshot",
        lua.create_function(move |lua, ()| {
            let owner = owner
                .lock()
                .map_err(|_| mlua::Error::runtime("settings owner mutex poisoned"))?;
            snapshot(lua, &owner)
        })?,
    )?;
    api.set(
        "reload",
        lua.create_function(move |lua, ()| {
            let mut owner = owner
                .lock()
                .map_err(|_| mlua::Error::runtime("settings owner mutex poisoned"))?;
            owner.reload().map_err(report)?;
            snapshot(lua, &owner)
        })?,
    )?;
    api.set(
        "set",
        lua.create_function(move |lua, (name, value): (String, LuaValue)| {
            let key = Key::parse(&name).map_err(report)?;
            let value = match value {
                LuaValue::Boolean(value) => Value::Bool(value),
                LuaValue::String(value) => Value::Text(value.to_str()?.to_owned()),
                LuaValue::Integer(value) => Value::Integer(value),
                LuaValue::Number(value)
                    if key == Key::CornerSize
                        && value.fract() == 0.0
                        && (1.0..=100.0).contains(&value) =>
                {
                    // Finite integral value in 1..100, validated before conversion.
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "validated integral range 1..100"
                    )]
                    let integer = value as i64;
                    Value::Integer(integer)
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "setting value must be a supported boolean, string or bounded integer",
                    ));
                }
            };
            let mut owner = owner
                .lock()
                .map_err(|_| mlua::Error::runtime("settings owner mutex poisoned"))?;
            let outcome = owner.set(key, value).map_err(report)?;
            applied(lua, &owner, outcome)
        })?,
    )?;
    lua.globals().set("tohru_settings", api)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_owner_survives_vm_replacement() {
        let root = std::env::temp_dir().join(format!("tohru-vm-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("tohru")).unwrap();
        let managed = root.join("managed.toml");
        std::fs::write(&managed, "[bar]\nmooncoil = false\n").unwrap();
        let owner = Owner::load(root.join("tohru/config.toml"), Some(managed.clone())).unwrap();
        assert!(SETTINGS.set(Mutex::new(owner)).is_ok());
        let lua = Lua::new();
        install(&lua).unwrap();
        lua.load("tohru_settings.set('bar.mooncoil', true)")
            .exec()
            .unwrap();
        drop(lua);
        let replacement = Lua::new();
        install(&replacement).unwrap();
        replacement.load("local s = tohru_settings.reload()['bar.mooncoil']; assert(s.value and s.managed and s.runtime_override and s.origin == 'runtime')").exec().unwrap();
        replacement.load("tohru_settings.set('appearance.corner_size', 100); assert(tohru_settings.reload()['appearance.corner_size'].value == 100); for _, n in {0, 101, 32.5, math.huge, 0/0} do assert(not pcall(tohru_settings.set, 'appearance.corner_size', n)) end; assert(tohru_settings.snapshot()['appearance.corner_size'].value == 100)").exec().unwrap();
        assert_eq!(
            std::fs::read_to_string(managed).unwrap(),
            "[bar]\nmooncoil = false\n"
        );
        let warning =
            SetOutcome::CommittedDurabilityUnconfirmed(tohru_shell::settings::DurabilityWarning {
                path: root.join("tohru/config.toml"),
                source: std::io::Error::other("injected directory fsync failure"),
            });
        let (snapshot, warning) = applied(
            &replacement,
            &SETTINGS.get().unwrap().lock().unwrap(),
            warning,
        )
        .unwrap();
        replacement
            .globals()
            .set("applied_snapshot", snapshot)
            .unwrap();
        replacement
            .globals()
            .set("applied_warning", warning)
            .unwrap();
        replacement.load("assert(applied_snapshot['bar.mooncoil'].value); assert(string.find(applied_warning, 'settings committed')); assert(string.find(applied_warning, 'crash durability is unconfirmed'))").exec().unwrap();
        let warning = replacement
            .globals()
            .get::<String>("applied_warning")
            .unwrap();
        assert_eq!(
            warning.matches("injected directory fsync failure").count(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
