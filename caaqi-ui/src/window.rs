//! Opening a window and drawing into it with vello.

use std::{num::NonZeroUsize, sync::Arc};

use pollster::FutureExt;
use vello::{
    AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene,
    peniko::color::palette,
    util::{RenderContext, RenderSurface},
    wgpu::{self, CurrentSurfaceTexture},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    error::{EventLoopError, OsError},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

/// Opens a window and runs until it's closed.
///
/// `draw` fills a fresh scene each time the window redraws. It's given the
/// window's size in physical pixels.
pub fn run(draw: impl FnMut(&mut Scene, PhysicalSize<u32>) + 'static) -> Result<(), RunError> {
    let event_loop = EventLoop::new()?;

    let mut app = App {
        draw,
        render_cx: RenderContext::new(),
        renderer: None,
        window: None,
        surface: None,
        scene: Scene::new(),
        error: None,
    };

    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

/// Why [`run`] stopped before its window was closed.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    EventLoop(#[from] EventLoopError),
    #[error(transparent)]
    CreateWindow(#[from] OsError),
    #[error(transparent)]
    Vello(#[from] vello::Error),
    #[error("the window's surface failed validation")]
    SurfaceValidation,
}

struct App<D> {
    draw: D,
    render_cx: RenderContext,
    /// The renderer for the current surface's device. Made with the first
    /// surface, and kept while later surfaces stay on the same device.
    renderer: Option<DeviceRenderer>,
    /// Kept while suspended, so resuming reuses it.
    window: Option<Arc<Window>>,
    /// Only exists while resumed.
    surface: Option<RenderSurface<'static>>,
    /// Reused between frames to keep its allocations.
    scene: Scene,
    /// The error that stopped the event loop, returned by [`run`].
    error: Option<RunError>,
}

/// A renderer, and the device it was made on.
///
/// `render_cx` keeps a list of GPU devices, and each surface's `dev_id` is
/// the index of the one it's drawn with. A renderer's shaders and buffers
/// live on one device, so it can only draw surfaces with the same `dev_id`.
struct DeviceRenderer {
    dev_id: usize,
    renderer: Renderer,
}

impl<D: FnMut(&mut Scene, PhysicalSize<u32>)> App<D> {
    fn create_surface(&mut self, event_loop: &ActiveEventLoop) -> Result<(), RunError> {
        let window = match &self.window {
            Some(window) => window.clone(),
            None => {
                let attributes = Window::default_attributes().with_title("caaqi");
                let window = Arc::new(event_loop.create_window(attributes)?);
                self.window = Some(window.clone());
                window
            }
        };
        let size = window.inner_size();

        let surface = self
            .render_cx
            .create_surface(
                window.clone(),
                size.width.max(1),
                size.height.max(1),
                wgpu::PresentMode::AutoVsync,
            )
            .block_on()?;

        // A new surface is almost always on the same device as the last one,
        // so the renderer is kept. It can land on another device on a machine
        // with several GPUs, for example when it's recreated after being lost,
        // and then the old renderer can't draw it, so it's replaced.
        let same_device = self
            .renderer
            .as_ref()
            .is_some_and(|renderer| renderer.dev_id == surface.dev_id);
        if !same_device {
            // Drop the old renderer first, to free its GPU memory.
            self.renderer = None;
            let device = &self.render_cx.devices[surface.dev_id].device;
            let renderer = Renderer::new(
                device,
                RendererOptions {
                    antialiasing_support: AaSupport::area_only(),
                    num_init_threads: NonZeroUsize::new(1),
                    ..RendererOptions::default()
                },
            )?;
            self.renderer = Some(DeviceRenderer {
                dev_id: surface.dev_id,
                renderer,
            });
        }

        self.surface = Some(surface);
        window.request_redraw();
        Ok(())
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        // A minimized window can report a zero size, which a surface can't have.
        if size.width == 0 || size.height == 0 {
            return;
        }
        if let Some(surface) = &mut self.surface {
            self.render_cx
                .resize_surface(surface, size.width, size.height);
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), RunError> {
        let Some(surface) = &mut self.surface else {
            return Ok(());
        };
        let (frame, reconfigure) = match surface.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => (frame, false),
            CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return Ok(()),
            CurrentSurfaceTexture::Outdated => {
                self.render_cx.configure_surface(surface);
                self.request_redraw();
                return Ok(());
            }
            CurrentSurfaceTexture::Lost => {
                self.surface = None;
                return self.create_surface(event_loop);
            }
            CurrentSurfaceTexture::Validation => return Err(RunError::SurfaceValidation),
        };

        let (width, height) = (surface.config.width, surface.config.height);
        self.scene.reset();
        (self.draw)(&mut self.scene, PhysicalSize::new(width, height));

        let device_handle = &self.render_cx.devices[surface.dev_id];
        let (device, queue) = (&device_handle.device, &device_handle.queue);
        let renderer = &mut self
            .renderer
            .as_mut()
            .filter(|renderer| renderer.dev_id == surface.dev_id)
            .expect("`create_surface` makes a renderer for the surface's device")
            .renderer;
        // Vello renders with compute shaders, which can't write to the
        // surface's texture, so it renders to its own and copies it over.
        renderer.render_to_texture(
            device,
            queue,
            &self.scene,
            &surface.target_view,
            &RenderParams {
                base_color: palette::css::BLACK,
                width,
                height,
                antialiasing_method: AaConfig::Area,
            },
        )?;
        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("caaqi-ui blit"),
        });
        surface
            .blitter
            .copy(device, &mut encoder, &surface.target_view, &frame_view);
        queue.submit([encoder.finish()]);
        if let Some(window) = &self.window {
            window.pre_present_notify();
        }
        queue.present(frame);

        // Reconfiguring while a frame is alive panics, so this waits until
        // it's presented.
        if reconfigure {
            self.render_cx.configure_surface(surface);
        }
        Ok(())
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: RunError) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl<D: FnMut(&mut Scene, PhysicalSize<u32>)> ApplicationHandler for App<D> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.surface.is_some() {
            return;
        }
        if let Err(error) = self.create_surface(event_loop) {
            self.fail(event_loop, error);
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.surface = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size),
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            _ => {}
        }
    }
}
