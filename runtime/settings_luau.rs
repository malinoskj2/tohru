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
            Value::WallpaperFit(value) => row.set("value", value.as_str())?,
            Value::WallpaperPreset(value) => row.set("value", value.map(|preset| preset.name()))?,
            Value::WallpaperChange(value) => row.set("value", value.name())?,
            Value::Palette(value) => row.set("value", value.name())?,
            Value::Integer(value) => row.set("value", value)?,
            Value::Assignments(assignments) => {
                row.set("value", lua.create_table_from(assignments)?)?
            }
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

fn input(key: Key, value: LuaValue) -> mlua::Result<Value> {
    Ok(match value {
        LuaValue::Boolean(value) => Value::Bool(value),
        LuaValue::Nil if key == Key::WallpaperPreset => Value::WallpaperPreset(None),
        LuaValue::String(value) => key.parse_text(value.to_str()?.as_ref()).map_err(report)?,
        LuaValue::Integer(value) => Value::Integer(value),
        LuaValue::Table(table) if key == Key::WallpaperAssignments => {
            let mut assignments = std::collections::BTreeMap::new();
            for pair in table.pairs::<LuaValue, LuaValue>() {
                if assignments.len() == 32 {
                    return Err(mlua::Error::runtime(
                        "wallpaper assignments exceed 32 outputs",
                    ));
                }
                let (LuaValue::String(output), LuaValue::String(path)) = pair? else {
                    return Err(mlua::Error::runtime(
                        "wallpaper assignments require string output/path pairs",
                    ));
                };
                assignments.insert(output.to_str()?.to_owned(), path.to_str()?.to_owned());
            }
            Value::Assignments(assignments)
        }
        LuaValue::Number(value)
            if value.fract() == 0.0
                && key.integer_bounds().is_some_and(|(min, max)| {
                    // All schema bounds fit exactly in f64 (currently <=4294967).
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "schema integer bounds fit exactly in f64"
                    )]
                    {
                        (min as f64..=max as f64).contains(&value)
                    }
                }) =>
        {
            // Finite integral value in the schema range, validated before conversion.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "validated finite integral schema range fits i64"
            )]
            let integer = value as i64;
            Value::Integer(integer)
        }
        _ => {
            return Err(mlua::Error::runtime(
                "setting value must be a supported boolean, string or bounded integer",
            ));
        }
    })
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
            let value = input(key, value)?;
            let mut owner = owner
                .lock()
                .map_err(|_| mlua::Error::runtime("settings owner mutex poisoned"))?;
            let outcome = owner.set(key, value).map_err(report)?;
            applied(lua, &owner, outcome)
        })?,
    )?;
    api.set(
        "wallpaper_update",
        lua.create_function(move |lua, proposal: Table| {
            let mut candidates = std::collections::BTreeMap::new();
            for pair in proposal.pairs::<String, LuaValue>() {
                let (name, value) = pair?;
                let key = match name.as_str() {
                    "assignments" => Key::WallpaperAssignments,
                    "preset" => Key::WallpaperPreset,
                    "folder" => Key::WallpaperFolder,
                    "palette" => Key::Palette,
                    _ => {
                        return Err(mlua::Error::runtime(
                            "unsupported wallpaper transaction field",
                        ));
                    }
                };
                candidates.insert(key, input(key, value)?);
            }
            let mut owner = owner
                .lock()
                .map_err(|_| mlua::Error::runtime("settings owner mutex poisoned"))?;
            let outcome = owner.set_many(candidates).map_err(report)?;
            if matches!(outcome, SetOutcome::RuntimeOverride) {
                return Ok((snapshot(lua, &owner)?, Some("This related wallpaper change is temporary because a participating setting is Nix managed or already temporary; none of it was saved.".to_owned())));
            }
            applied(lua, &owner, outcome)
        })?,
    )?;
    api.set(
        "wallpaper_assign",
        lua.create_function(move |lua, (output, path): (String, Option<String>)| {
            let mut owner = owner
                .lock()
                .map_err(|_| mlua::Error::runtime("settings owner mutex poisoned"))?;
            let outcome = owner
                .assign_wallpaper(&output, path.as_deref())
                .map_err(report)?;
            applied(lua, &owner, outcome)
        })?,
    )?;
    lua.globals().set("tohru_settings", api)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tohru_shell::settings::WallpaperFit;

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
        lua.load("tohru_settings.set('wallpaper.fit', 'centre')")
            .exec()
            .unwrap();
        drop(lua);
        let replacement = Lua::new();
        install(&replacement).unwrap();
        replacement.load(r#"
            assert(tohru_settings.snapshot()['wallpaper.fit'].value=='centre')
            for _,mode in ipairs({'fill','fit','centre','tile'}) do
                local updated=tohru_settings.set('wallpaper.fit',mode)
                assert(type(updated['wallpaper.fit'].value)=='string' and updated['wallpaper.fit'].value==mode)
                assert(tohru_settings.reload()['wallpaper.fit'].value==mode)
            end
        "#).exec().unwrap();
        let fit_disk = std::fs::read(root.join("tohru/config.toml")).unwrap();
        replacement
            .load(
                r#"
            for _,invalid in ipairs({'cover','contain','center','Fill','tile ','',true,1,{}}) do
                assert(not pcall(tohru_settings.set,'wallpaper.fit',invalid))
                assert(tohru_settings.snapshot()['wallpaper.fit'].value=='tile')
            end
        "#,
            )
            .exec()
            .unwrap();
        assert_eq!(
            std::fs::read(root.join("tohru/config.toml")).unwrap(),
            fit_disk
        );
        assert_eq!(
            SETTINGS
                .get()
                .unwrap()
                .lock()
                .unwrap()
                .get(Key::WallpaperFit)
                .value,
            Value::WallpaperFit(WallpaperFit::Tile)
        );
        replacement.load("local s = tohru_settings.reload()['bar.mooncoil']; assert(s.value and s.managed and s.runtime_override and s.origin == 'runtime')").exec().unwrap();
        replacement.load("tohru_settings.set('appearance.corner_size', 100); assert(tohru_settings.reload()['appearance.corner_size'].value == 100); for _, n in {0, 101, 32.5, math.huge, 0/0} do assert(not pcall(tohru_settings.set, 'appearance.corner_size', n)) end; assert(tohru_settings.snapshot()['appearance.corner_size'].value == 100)").exec().unwrap();
        replacement.load("tohru_settings.set('wallpaper.assignments', {OFFLINE='/offline.png'}); tohru_settings.wallpaper_assign('DP-1', '/new.webp'); local s=tohru_settings.reload()['wallpaper.assignments']; assert(s.value.OFFLINE=='/offline.png' and s.value['DP-1']=='/new.webp'); assert(not pcall(tohru_settings.set, 'wallpaper.assignments', {[1]='/bad.png'})); assert(not pcall(tohru_settings.set, 'wallpaper.assignments', {BAD=false})); assert(not pcall(tohru_settings.wallpaper_assign, ' DP-1', '/bad.png')); tohru_settings.wallpaper_assign('DP-1', nil); assert(tohru_settings.snapshot()['wallpaper.assignments'].value['DP-1']==nil)").exec().unwrap();
        replacement.load("tohru_settings.set('wallpaper.folder', '/offline/壁 紙'); tohru_settings.set('capture.copy', false); tohru_settings.set('capture.directory', '~/Pictures'); local s=tohru_settings.reload(); assert(s['wallpaper.folder'].value=='/offline/壁 紙' and s['capture.copy'].value==false and s['capture.directory'].value=='~/Pictures'); assert(not pcall(tohru_settings.set, 'capture.directory', '../invalid')); assert(not pcall(tohru_settings.set, 'capture.filename_pattern', '../escape')); assert(tohru_settings.snapshot()['capture.directory'].value=='~/Pictures')").exec().unwrap();
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
        replacement.load(r#"
            local s=tohru_settings.wallpaper_update({assignments={A='/solid.png'},preset='solid',palette='nord',folder='/chosen'})
            assert(s['wallpaper.preset'].value=='solid' and s['appearance.palette'].value=='nord')
            assert(s['wallpaper.assignments'].value.A=='/solid.png' and s['wallpaper.folder'].value=='/chosen')
            for _,bad in ipairs({{palette='Nord'},{preset='unknown'},{assignments=false},{unrelated=true}})do
                assert(not pcall(tohru_settings.wallpaper_update,bad))
                assert(tohru_settings.snapshot()['wallpaper.assignments'].value.A=='/solid.png')
            end
        "#).exec().unwrap();
        std::fs::write(
            root.join("managed.toml"),
            "[bar]\nmooncoil=false\n[appearance]\npalette='mocha'\n",
        )
        .unwrap();
        replacement.load("tohru_settings.reload()").exec().unwrap();
        let disk = std::fs::read(root.join("tohru/config.toml")).unwrap();
        replacement.load(r#"
            local s,warning=tohru_settings.wallpaper_update({assignments={A='/temporary.png'},preset='dusk',palette='nord'})
            assert(warning:find('none of it was saved'))
            assert(s['wallpaper.preset'].runtime_override and s['wallpaper.assignments'].runtime_override)
            assert(not s['wallpaper.assignments'].managed and s['appearance.palette'].managed)
            assert(tohru_settings.reload()['wallpaper.assignments'].value.A=='/temporary.png')
        "#).exec().unwrap();
        assert_eq!(std::fs::read(root.join("tohru/config.toml")).unwrap(), disk);
        std::fs::remove_dir_all(root).unwrap();
    }
}
