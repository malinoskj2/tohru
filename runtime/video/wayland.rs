//! A single video-only Wayland connection, SCTK output/layer lifetime and frame permission.
#![expect(
    unsafe_code,
    reason = "poll only owned Wayland and coalesced wake descriptors"
)]
use std::collections::BTreeMap;
use std::io;
use std::os::fd::{AsFd, AsRawFd};
use std::rc::Rc;
use std::time::{Duration, Instant};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
    delegate_registry,
    dispatch2::Dispatch2,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use tohru_shell::video::{OutputPixels, Snapshot};
use wayland_client::{
    Connection, EventQueue, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

use super::wake::Receiver;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("video Wayland {stage}")]
    Protocol {
        stage: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("duplicate assigned output names in compositor metadata")]
    Duplicate,
    #[error("video output has no valid configured logical geometry")]
    Geometry,
    #[error("video output has no layer")]
    MissingLayer,
    #[error("polling owned video connection")]
    Poll(#[from] io::Error),
}
fn protocol(stage: &'static str, source: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::Protocol {
        stage,
        source: Box::new(source),
    }
}

pub(super) struct Viewport(WpViewport);
impl Viewport {
    pub(super) fn destination(&self, w: i32, h: i32) {
        self.0.set_destination(w, h);
    }
}
impl Drop for Viewport {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

pub(super) struct LayerRecord {
    // Viewport before layer so its final protocol destruction precedes wl_surface.
    pub viewport: Option<Rc<Viewport>>,
    pub layer: LayerSurface,
    pub output: wl_output::WlOutput,
    pub configured: Option<(u32, u32)>,
    pub frame_ready: bool,
    pub closed: bool,
    pub created: Instant,
}

pub(super) struct Connected {
    pub output: wl_output::WlOutput,
    pub pixels: OutputPixels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Geometry {
    pub logical: (u32, u32),
    pub physical: (u32, u32),
    pub scale: i32,
}

pub(super) struct State {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    shell: LayerShell,
    viewporter: Option<WpViewporter>,
    pub layers: BTreeMap<String, LayerRecord>,
}

pub(super) struct Wayland {
    // Drop layer roles and registry before queue/connection. EGL/MPV owners are
    // explicitly retired before this object by the containing video controller.
    pub state: State,
    pub queue: EventQueue<State>,
    pub connection: Connection,
}

impl Wayland {
    /// The containing controller publishes this sole connection before calling
    /// SCTK bootstrap and owns its startup deadline/cancellation watchdog. No GL
    /// context may exist until that controller marks bootstrap complete.
    pub(super) fn initialize(connection: Connection) -> Result<Self, Error> {
        let (globals, queue) = registry_queue_init(&connection)
            .map_err(|error| protocol("registry bootstrap", error))?;
        let handle = queue.handle();
        let compositor = CompositorState::bind(&globals, &handle)
            .map_err(|error| protocol("bind compositor", error))?;
        let shell = LayerShell::bind(&globals, &handle)
            .map_err(|error| protocol("bind layer shell", error))?;
        let viewporter = globals.bind(&handle, 1..=1, Ignore).ok();
        let state = State {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &handle),
            compositor,
            shell,
            viewporter,
            layers: BTreeMap::new(),
        };
        Ok(Self {
            state,
            queue,
            connection,
        })
    }

    pub(super) fn dispatch(&mut self) -> Result<(), Error> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|error| protocol("dispatch", error))?;
        Ok(())
    }

    pub(super) fn connected(
        &self,
        snapshot: &Snapshot,
    ) -> Result<BTreeMap<String, Connected>, Error> {
        let mut connected = BTreeMap::new();
        for output in self.state.outputs.outputs() {
            let Some(info) = self.state.outputs.info(&output) else {
                continue;
            };
            let Some(name) = info.name else { continue };
            if !snapshot.assignments.contains_key(&name) {
                continue;
            }
            let transform: u32 = info.transform.into();
            let pixels = OutputPixels {
                logical: info.logical_size.and_then(positive),
                mode: info
                    .modes
                    .iter()
                    .find(|mode| mode.current)
                    .and_then(|mode| positive(mode.dimensions)),
                rotated: transform & 1 == 1,
                integer_scale: u32::try_from(info.scale_factor).unwrap_or(0),
            };
            if connected
                .insert(name, Connected { output, pixels })
                .is_some()
            {
                return Err(Error::Duplicate);
            }
        }
        Ok(connected)
    }

    pub(super) fn create_layer(
        &mut self,
        name: &str,
        output: wl_output::WlOutput,
        now: Instant,
    ) -> Result<(), Error> {
        let qh = self.queue.handle();
        let surface = self.state.compositor.create_surface(&qh);
        let empty = Region::new(&self.state.compositor)
            .map_err(|error| protocol("empty input region", error))?;
        surface.set_input_region(Some(empty.wl_region()));
        surface.set_buffer_transform(wl_output::Transform::Normal);
        let viewport = self
            .state
            .viewporter
            .as_ref()
            .map(|manager| Rc::new(Viewport(manager.get_viewport(&surface, &qh, Ignore))));
        let layer = self.state.shell.create_layer_surface(
            &qh,
            surface,
            Layer::Background,
            Some("tohru-wallpaper-video"),
            Some(&output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        self.state.layers.insert(
            name.to_owned(),
            LayerRecord {
                viewport,
                layer: layer.clone(),
                output,
                configured: None,
                frame_ready: true,
                closed: false,
                created: now,
            },
        );
        layer.commit();
        Ok(())
    }

    pub(super) fn geometry(
        &self,
        name: &str,
        connected: &Connected,
    ) -> Result<Option<Geometry>, Error> {
        let layer = self.state.layers.get(name).ok_or(Error::MissingLayer)?;
        let Some(configured) = layer.configured else {
            return Ok(None);
        };
        let fallback = connected.pixels.logical.unwrap_or((0, 0));
        let logical = (
            if configured.0 == 0 {
                fallback.0
            } else {
                configured.0
            },
            if configured.1 == 0 {
                fallback.1
            } else {
                configured.1
            },
        );
        if logical.0 == 0 || logical.1 == 0 {
            return Err(Error::Geometry);
        }
        let mut pixels = connected.pixels;
        // This role has all anchors, zero requested size/margins and reserve -1:
        // its configured box is the output's actual logical extent even when
        // xdg-output logical_size metadata is unavailable.
        pixels.logical = pixels.logical.or(Some(logical));
        let physical = pixels
            .size(logical, layer.viewport.is_some())
            .map_err(|_| Error::Geometry)?;
        let scale = i32::try_from(connected.pixels.integer_scale).map_err(|_| Error::Geometry)?;
        Ok(Some(Geometry {
            logical,
            physical,
            scale,
        }))
    }

    pub(super) fn request_frame(&mut self, name: &str) -> Result<(), Error> {
        let layer = self.state.layers.get_mut(name).ok_or(Error::MissingLayer)?;
        layer.frame_ready = false;
        let surface = layer.layer.wl_surface();
        surface.frame(&self.queue.handle(), FrameCallbackData(surface.clone()));
        Ok(())
    }

    pub(super) fn wait(&self, wake: &Receiver, timeout: Option<Duration>) -> Result<(), Error> {
        let wants_write = match self.connection.flush() {
            Ok(()) => false,
            Err(wayland_client::backend::WaylandError::Io(error))
                if error.kind() == io::ErrorKind::WouldBlock =>
            {
                true
            }
            Err(error) => return Err(protocol("flush", error)),
        };
        let Some(read) = self.queue.prepare_read() else {
            return Ok(());
        };
        let mut fds = [
            libc::pollfd {
                fd: read.connection_fd().as_raw_fd(),
                events: libc::POLLIN | if wants_write { libc::POLLOUT } else { 0 },
                revents: 0,
            },
            libc::pollfd {
                fd: wake.as_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let timeout = timeout.map_or(-1, |duration| {
            i32::try_from(duration.as_millis().saturating_add(1)).unwrap_or(i32::MAX)
        });
        // SAFETY: the two borrowed owned descriptors and pollfd storage stay live for this call.
        let result = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                return Ok(());
            }
            return Err(error.into());
        }
        if fds
            .iter()
            .any(|fd| fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "owned video connection or wake closed",
            )
            .into());
        }
        if fds[0].revents & libc::POLLIN != 0 {
            match read.read() {
                Ok(_) => {}
                Err(wayland_client::backend::WaylandError::Io(error))
                    if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(protocol("read", error)),
            }
        }
        Ok(())
    }
}

fn positive(value: (i32, i32)) -> Option<(u32, u32)> {
    let value = (u32::try_from(value.0).ok()?, u32::try_from(value.1).ok()?);
    (value.0 > 0 && value.1 > 0).then_some(value)
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(layer) = self
            .layers
            .values_mut()
            .find(|layer| layer.layer.wl_surface() == surface)
        {
            layer.frame_ready = true;
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}
impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        for layer in self
            .layers
            .values_mut()
            .filter(|layer| layer.output == output)
        {
            layer.closed = true;
        }
    }
}
impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &LayerSurface) {
        if let Some(layer) = self
            .layers
            .values_mut()
            .find(|layer| layer.layer == *surface)
        {
            layer.closed = true;
        }
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let Some(layer) = self
            .layers
            .values_mut()
            .find(|layer| layer.layer == *surface)
        {
            layer.configured = Some(configure.new_size);
        }
    }
}

struct Ignore;
impl Dispatch2<WpViewporter, State> for Ignore {
    fn event(
        &self,
        _: &mut State,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}
impl Dispatch2<WpViewport, State> for Ignore {
    fn event(
        &self,
        _: &mut State,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}
delegate_registry!(State);
impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}
smithay_client_toolkit::delegate_dispatch2!(State);
