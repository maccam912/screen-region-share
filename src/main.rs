use anyhow::{Context, Result};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use log::{error, info, warn};
use pixels::{Pixels, SurfaceTexture};
use std::time::{Duration, Instant};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};
use xcap::Monitor;

const DEFAULT_WIDTH: u32 = 640;
const DEFAULT_HEIGHT: u32 = 480;
const DEBOUNCE_DURATION: Duration = Duration::from_millis(300); // Add debounce duration

#[derive(PartialEq)]
enum WindowMode {
    Alignment,
    Share,
}

struct App {
    monitor: Monitor,
    pixels: Pixels,
    window_inner_pos: (i32, i32),
    width: u32,
    height: u32,
    mode: WindowMode,
    last_toggle: Instant,
}

impl App {
    fn new(window: &Window) -> Result<Self> {
        // Get primary monitor
        let monitors = Monitor::all().context("Couldn't get monitors")?;
        let monitor = monitors.into_iter().next().context("No monitors found")?;

        // Create a surface for rendering
        let size = window.inner_size();
        let width = size.width;
        let height = size.height;
        let surface_texture = SurfaceTexture::new(width, height, window);
        let pixels = Pixels::new(width, height, surface_texture)
            .context("Failed to create pixels instance")?;

        // Get initial window inner position as physical pixels
        let window_inner_pos = window.inner_position().unwrap_or_default();

        Ok(Self {
            monitor,
            pixels,
            window_inner_pos: (window_inner_pos.x, window_inner_pos.y),
            width,
            height,
            mode: WindowMode::Alignment,
            last_toggle: Instant::now(),
        })
    }

    fn toggle_mode(&mut self, window: &Window) {
        let now = Instant::now();
        if now.duration_since(self.last_toggle) < DEBOUNCE_DURATION {
            return; // Ignore toggle if within debounce period
        }
        self.last_toggle = now;

        match self.mode {
            WindowMode::Alignment => {
                window.set_decorations(false);
                window.set_content_protected(false);
                window.set_window_level(winit::window::WindowLevel::AlwaysOnBottom);
                self.mode = WindowMode::Share;
                info!("Switched to share mode");
            }
            WindowMode::Share => {
                window.set_decorations(true);
                window.set_content_protected(true);
                window.set_window_level(winit::window::WindowLevel::AlwaysOnTop);
                self.mode = WindowMode::Alignment;
                info!("Switched to alignment mode");
            }
        }
    }

    fn update(&mut self, window: &Window) -> Result<()> {
        // Update window position if changed - use inner position for content area
        if let Ok(position) = window.inner_position() {
            self.window_inner_pos = (position.x, position.y);
        }

        // Capture screen region and update pixels
        self.capture_and_update()?;

        Ok(())
    }

    fn capture_and_update(&mut self) -> Result<()> {
        // Capture the screen using xcap
        let captured_image = self
            .monitor
            .capture_image()
            .context("Failed to capture screen")?;

        let width = self.width;
        let height = self.height;
        let frame = self.pixels.frame_mut();

        // Get inner window position in physical pixels
        let window_x = self.window_inner_pos.0;
        let window_y = self.window_inner_pos.1;

        // Get monitor dimensions - handle Results properly
        let display_width = self
            .monitor
            .width()
            .context("Failed to get monitor width")? as usize;
        let display_height = self
            .monitor
            .height()
            .context("Failed to get monitor height")? as usize;

        // Get image dimensions and raw RGBA pixels
        let image_width = captured_image.width() as usize;
        let pixels_data = captured_image.as_raw(); // Gets raw pixel data

        for y in 0..height as usize {
            for x in 0..width as usize {
                // Calculate the corresponding position in the captured screen
                // Using inner_position for accurate content area positioning
                let screen_x = if window_x < 0 {
                    x.saturating_sub(window_x.unsigned_abs() as usize)
                } else {
                    x.saturating_add(window_x as usize)
                };

                let screen_y = if window_y < 0 {
                    y.saturating_sub(window_y.unsigned_abs() as usize)
                } else {
                    y.saturating_add(window_y as usize)
                };

                // Skip if outside capture area
                if screen_x >= display_width || screen_y >= display_height {
                    continue;
                }

                // Calculate buffer indices safely with checked math
                // image uses RGBA format, 4 bytes per pixel
                if let Some(buffer_idx) = screen_y
                    .checked_mul(image_width * 4)
                    .and_then(|v| v.checked_add(screen_x * 4))
                {
                    if buffer_idx + 3 < pixels_data.len() {
                        let r = pixels_data[buffer_idx];
                        let g = pixels_data[buffer_idx + 1];
                        let b = pixels_data[buffer_idx + 2];
                        let a = pixels_data[buffer_idx + 3];

                        // Set pixel in our frame (RGBA format for pixels crate)
                        if let Some(frame_idx) = y
                            .checked_mul(width as usize)
                            .and_then(|v| v.checked_add(x))
                            .map(|v| v * 4)
                        {
                            if frame_idx + 3 < frame.len() {
                                frame[frame_idx] = r;
                                frame[frame_idx + 1] = g;
                                frame[frame_idx + 2] = b;
                                frame[frame_idx + 3] = a;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn render(&mut self) -> Result<()> {
        self.pixels.render().context("Render error")?;
        Ok(())
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.width = new_size.width;
            self.height = new_size.height;

            // Handle the Result from resize operations
            if let Err(e) = self.pixels.resize_surface(new_size.width, new_size.height) {
                warn!("Failed to resize surface: {}", e);
            }

            if let Err(e) = self.pixels.resize_buffer(new_size.width, new_size.height) {
                warn!("Failed to resize buffer: {}", e);
            }
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    info!("Starting screen region share application");

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Screen Region Share")
        .with_inner_size(PhysicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT))
        .with_min_inner_size(PhysicalSize::new(100, 100))
        .with_decorations(true)
        .with_content_protected(true)
        .build(&event_loop)?;

    // Initialize hotkey manager
    let manager = GlobalHotKeyManager::new().context("Failed to create hotkey manager")?;
    let hotkey = HotKey::new(
        Some(Modifiers::SHIFT | Modifiers::CONTROL),
        Code::BracketLeft,
    );
    manager
        .register(hotkey)
        .context("Failed to register hotkey")?;

    let mut app = App::new(&window)?;
    let mut last_update = Instant::now();
    let frame_rate = Duration::from_millis(1000 / 30); // 30 FPS

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        // Check for hotkey events
        if let Ok(_) = GlobalHotKeyEvent::receiver().try_recv() {
            app.toggle_mode(&window);
        }

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                info!("Window close requested");
                *control_flow = ControlFlow::Exit;
            }

            Event::WindowEvent {
                event: WindowEvent::Resized(new_size),
                ..
            } => {
                app.resize(new_size);
            }

            Event::MainEventsCleared => {
                window.request_redraw();
            }

            Event::RedrawRequested(_) => {
                let now = Instant::now();
                if now - last_update >= frame_rate {
                    last_update = now;

                    if let Err(err) = app.update(&window) {
                        error!("Update error: {}", err);
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }

                if let Err(err) = app.render() {
                    error!("Render error: {}", err);
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }

            _ => {}
        }
    });
}
