//! Native root-locker bridge. Generic PAM queries never issue release permits.
#[path = "tohru_lock/authority.rs"]
pub(crate) mod authority;
#[path = "tohru_lock/publication.rs"]
pub(crate) mod publication;

pub(crate) use authority::{Cycle, Permit};

pub(crate) fn authenticate_unlock(
    root: bool,
    password: &str,
    render: &std::sync::mpsc::Sender<crate::scene::ToRender>,
) -> anyhow::Result<bool> {
    anyhow::ensure!(root, "only the native root locker may authenticate release");
    // The snapshot is native, confirmed and captured BEFORE blocking PAM.
    let snapshot = crate::platform::lock_snapshot()
        .ok_or_else(|| anyhow::anyhow!("there is no confirmed native lock to authenticate"))?;
    let accepted = crate::platform::lock_password(password).map_err(anyhow::Error::msg)?;
    if !accepted {
        return Ok(false);
    }
    let permit = snapshot
        .verified(true)
        .ok_or_else(|| anyhow::anyhow!("the native lock changed during authentication"))?;
    render
        .send(crate::scene::ToRender::AuthenticatedUnlock(permit))
        .map_err(|_| anyhow::anyhow!("the native locker renderer has stopped"))?;
    Ok(true)
}

/// Active native owners override public presentation state consistently across
/// culling, input exclusion and surface pacing. Requested state is not authority.
pub(crate) fn surface_open(owner: usize, requested: bool, active: &[usize]) -> bool {
    active.contains(&owner) || requested
}

pub(crate) fn view_origin(bounds: (u32, u32), origin: (f32, f32), size: (f32, f32)) -> (f32, f32) {
    (
        origin.0 - (size.0 - bounds.0 as f32) / 2.0,
        origin.1 - (size.1 - bounds.1 as f32) / 2.0,
    )
}

/// Native secret metadata is process-retained by the compiler across reloads.
/// A public binding that later becomes secret must not regain its old export path.
pub(crate) fn secret(name: &str) -> bool {
    crate::scene::SECRETS
        .lock()
        .map_or(true, |secrets| secrets.contains(&name))
}

#[cfg(test)]
#[path = "../tests/session/native_security.rs"]
mod tests;
