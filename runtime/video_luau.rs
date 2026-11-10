use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use mlua::{Lua, Table};
use tohru_shell::video::{MAX_OUTPUTS, Snapshot};

use crate::scene::ToRender;

// Process-wide so a logic reload keeps the video observation revision increasing.
static REVISION: AtomicU64 = AtomicU64::new(0);

/// Root-only: the accepted whole-map video projection from the settings owner.
/// Protection and occupancy stay Unknown until verified providers exist, which
/// the policy treats as paused.
pub(crate) fn install(lua: &Lua, changed: Sender<ToRender>) -> mlua::Result<()> {
    let api = lua.create_table()?;
    api.set(
        "publish",
        lua.create_function(move |_, (enabled, assignments): (bool, Table)| {
            let mut map = BTreeMap::new();
            for pair in assignments.pairs::<String, String>() {
                let (output, path) = pair?;
                if map.len() == MAX_OUTPUTS {
                    return Err(mlua::Error::runtime("video assignments exceed 32 outputs"));
                }
                map.insert(output, path);
            }
            let snapshot = Snapshot {
                revision: REVISION.fetch_add(1, Ordering::Relaxed) + 1,
                enabled,
                assignments: map,
                ..Snapshot::default()
            };
            crate::tohru_video::publish(snapshot, changed.clone()).map_err(mlua::Error::runtime)
        })?,
    )?;
    lua.globals().set("tohru_video", api)
}
