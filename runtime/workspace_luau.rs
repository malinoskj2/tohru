//! Narrow publication boundary; exact simulation stays outside reloadable Luau.
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use mlua::{Lua, Table};
use tohru_shell::workspace::{Activity, Indicator, Sample};

struct Indicators {
    workspace: Indicator,
    calendar: Indicator,
}

impl Default for Indicators {
    fn default() -> Self {
        Self {
            workspace: Indicator::default(),
            calendar: Indicator::calendar(),
        }
    }
}

enum Role {
    Workspace,
    Calendar,
}

impl Role {
    fn parse(name: &str) -> mlua::Result<Self> {
        match name {
            "workspace" => Ok(Self::Workspace),
            "calendar" => Ok(Self::Calendar),
            _ => Err(mlua::Error::runtime(
                "indicator role must be workspace or calendar",
            )),
        }
    }

    fn select(self, indicators: &mut Indicators) -> &mut Indicator {
        match self {
            Self::Workspace => &mut indicators.workspace,
            Self::Calendar => &mut indicators.calendar,
        }
    }
}

static INDICATORS: OnceLock<Mutex<Indicators>> = OnceLock::new();

#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

fn publish(lua: &Lua, sample: Sample) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    result.set("shape", lua.create_sequence_from(sample.shape)?)?;
    result.set("color", lua.create_sequence_from(sample.color)?)?;
    result.set("width", sample.width)?;
    result.set("moving", sample.activity == Activity::Animating)?;
    Ok(result)
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let indicators = INDICATORS.get_or_init(Mutex::default);
    let api = lua.create_table()?;
    api.set(
        "observe",
        lua.create_function(
            move |lua, (role, highest, active): (String, Option<i64>, Option<usize>)| {
                let role = Role::parse(&role)?;
                let mut indicators = indicators
                    .lock()
                    .map_err(|_| mlua::Error::runtime("indicator mutex poisoned"))?;
                let indicator = role.select(&mut indicators);
                indicator.observe(highest, active, Instant::now());
                publish(lua, indicator.sample())
            },
        )?,
    )?;
    api.set(
        "tick",
        lua.create_function(move |lua, role: String| {
            let role = Role::parse(&role)?;
            let mut indicators = indicators
                .lock()
                .map_err(|_| mlua::Error::runtime("indicator mutex poisoned"))?;
            publish(lua, role.select(&mut indicators).advance(Instant::now()))
        })?,
    )?;
    api.set(
        "reset",
        lua.create_function(move |lua, role: String| {
            let role = Role::parse(&role)?;
            let mut indicators = indicators
                .lock()
                .map_err(|_| mlua::Error::runtime("indicator mutex poisoned"))?;
            let indicator = match role {
                Role::Workspace => {
                    indicators.workspace = Indicator::default();
                    &indicators.workspace
                }
                Role::Calendar => {
                    indicators.calendar = Indicator::calendar();
                    &indicators.calendar
                }
            };
            publish(lua, indicator.sample())
        })?,
    )?;
    lua.globals().set("tohru_workspace", api)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_and_momentum_survive_vm_replacement_without_a_timer_owner() {
        let _guard = TEST_LOCK.lock().unwrap();
        let lua = Lua::new();
        install(&lua).unwrap();
        lua.load("tohru_workspace.reset('workspace'); tohru_workspace.reset('calendar')")
            .exec()
            .unwrap();
        lua.load("local c=tohru_workspace.observe('calendar', nil, 12); assert(#c.shape==16 and #c.color==4 and c.width==192 and not c.moving); local w=tohru_workspace.observe('workspace',5,1); assert(w.width==80 and not w.moving); assert(tohru_workspace.observe('workspace',5,2).moving); assert(not pcall(tohru_workspace.tick,'unknown')); assert(not pcall(tohru_workspace.observe,'workspace',5,-1))").exec().unwrap();
        let before = INDICATORS.get().unwrap().lock().unwrap().workspace.sample();
        drop(lua);
        let replacement = Lua::new();
        install(&replacement).unwrap();
        let retained = INDICATORS.get().unwrap().lock().unwrap().workspace.sample();
        assert_eq!(before, retained);
        replacement.load("local c=tohru_workspace.tick('calendar'); assert(c.width==192 and not c.moving); local w=tohru_workspace.tick('workspace'); assert(w.moving and w.width==80 and #w.shape==16)").exec().unwrap();
        replacement.load("tohru_workspace.observe('calendar',12,1); assert(tohru_workspace.observe('calendar',12,12).moving); local c=tohru_workspace.reset('calendar'); assert(c.width==192 and not c.moving and c.color[4]==0); assert(not tohru_workspace.observe('calendar',12,11).moving); assert(tohru_workspace.tick('workspace').moving); assert(not pcall(tohru_workspace.reset,'unknown'))").exec().unwrap();
    }
}
