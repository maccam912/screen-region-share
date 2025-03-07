#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use anyhow::{Context, Result};
use log::{error, info, warn};
use pixels::{Pixels, SurfaceTexture};
use scrap::{Capturer, Display};
use std::io::ErrorKind::WouldBlock;
use std::time::{Duration, Instant};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

// Windows API imports
#[cfg(target_os = "windows")]
use windows::{
    Win32::Foundation::HWND,
    Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE},
};
#[cfg(target_os = "windows")]
use winit::platform::windows::WindowExtWindows;

// macOS imports
#[cfg(target_os = "macos")]
use {
    cocoa::appkit::NSWindowSharingNone,
    cocoa::base::nil,
    objc::runtime::Object,
    winit::platform::macos::WindowExtMacOS,
};

const DEFAULT_WIDTH: u32 = 640;
const DEFAULT_HEIGHT: u32 = 480;

// Helper function to exclude window from screen capture on Windows
#[cfg(target_os = "windows")]
fn exclude_window_from_capture(window: &Window) {
    info!("Setting WDA_EXCLUDEFROMCAPTURE on window");
    unsafe {
        let hwnd = HWND(window.hwnd() as isize);
        let result = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        if !result.as_bool() {
            warn!("Failed to set WDA_EXCLUDEFROMCAPTURE");
        }
    }
}

// Helper function to exclude window from screen capture on macOS
#[cfg(target_os = "macos")]
fn exclude_window_from_capture(window: &Window) {
    info!("Setting NSWindowSharingNone on window");
    unsafe {
        let ns_window = window.ns_window() as *mut Object;
        if ns_window != nil {
            let _: () = msg_send![ns_window, setShareTypes: NSWindowSharingNone];
        } else {
            warn!("Failed to get NSWindow reference");
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn exclude_window_from_capture(_window: &Window) {
    // Do nothing on unsupported platforms
    warn!("Window exclusion from capture is not supported on this platform");
}

struct App {
    capturer: Capturer,
    display_info: Display,
    pixels: Pixels,
    window_pos: (i32, i32),
    width: u32,
    height: u32,
}

impl App {
    fn new(window: &Window) -> Result<Self> {
        let display = Display::primary().context("Couldn't find primary display")?;
        let capturer = Capturer::new(display).context("Couldn't begin capture")?;
        
        // Create a surface for rendering
        let size = window.inner_size();
        let width = size.width;
        let height = size.height;
        let surface_texture = SurfaceTexture::new(width, height, window);
        let pixels = Pixels::new(width, height, surface_texture)
            .context("Failed to create pixels instance")?;

        // Get initial window position - use inner_position for client area only
        let window_pos = window.inner_position()
            .unwrap_or_default()
            .into();
            
        // Get the primary display again for reference
        let display_info = Display::primary().context("Couldn't get primary display info")?;

        Ok(Self {
            capturer,
            display_info,
            pixels,
            window_pos,
            width,
            height,
        })
    }

    fn update(&mut self, window: &Window) -> Result<()> {
        // Update window position if changed - use inner_position for client area only
        if let Ok(position) = window.inner_position() {
            self.window_pos = position.into();
        }
        
        // Capture screen region and update pixels
        self.capture_and_update()?;
        
        Ok(())
    }

    fn capture_and_update(&mut self) -> Result<()> {
        // Get capturer dimensions first before the mutable borrow
        let capturer_width = self.capturer.width();
        let stride = capturer_width as usize * 4; // 4 bytes per pixel (BGRA)
        
        // Try to get a frame
        let buffer = match self.capturer.frame() {
            Ok(buffer) => buffer,
            Err(error) => {
                if error.kind() == WouldBlock {
                    // Not ready yet, try again later
                    return Ok(());
                } else {
                    return Err(error.into());
                }
            }
        };

        let width = self.width;
        let height = self.height;
        let frame = self.pixels.frame_mut();
        
        // Calculate the actual position on the screen where the window is
        let (window_x, window_y) = self.window_pos;
        
        // Get capturer frame dimensions
        let display_width = self.display_info.width() as usize;
        let display_height = self.display_info.height() as usize;

        for y in 0..height as usize {
            for x in 0..width as usize {
                // Calculate the corresponding position in the captured screen
                // Use saturating_add to prevent overflow
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
                if let Some(buffer_idx) = screen_y.checked_mul(stride).and_then(|v| v.checked_add(screen_x * 4)) {
                    if buffer_idx + 3 < buffer.len() {
                        let b = buffer[buffer_idx];
                        let g = buffer[buffer_idx + 1];
                        let r = buffer[buffer_idx + 2];
                        let a = buffer[buffer_idx + 3];
                        
                        // Set pixel in our frame (RGBA format for pixels crate)
                        if let Some(frame_idx) = y.checked_mul(width as usize).and_then(|v| v.checked_add(x)).map(|v| v * 4) {
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
        .build(&event_loop)?;
    
    // Exclude window from screen capture
    exclude_window_from_capture(&window);
    
    let mut app = App::new(&window)?;
    let mut last_update = Instant::now();
    let frame_rate = Duration::from_millis(1000 / 30); // 30 FPS

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        
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
