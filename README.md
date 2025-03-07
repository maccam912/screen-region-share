# Screen Region Share

A simple utility that helps you share a specific region of your screen while protecting your privacy.

## How It Works

1. Launch the application - a window will appear
2. Position and resize the window to frame the region you want to share
3. Use `Ctrl+Shift+[` to toggle between alignment mode and sharing mode:
   - **Alignment Mode**: Window is visible and can be repositioned/resized
   - **Sharing Mode**: Window becomes invisible and captures the framed region

## Usage

1. Start the application
2. Move and resize the window to frame the exact region you want to share
3. Open your preferred screen sharing application (Zoom, Teams, etc.)
4. Select the "Screen Region Share" window as your sharing source
5. Use `Ctrl+Shift+[` to switch between modes:
   - When aligning: You'll see the window frame to help you position it
   - When sharing: The window frame disappears, showing only the content

Note: In alignment mode, others won't see your screen content - this is a privacy feature that prevents accidental sharing while you're positioning the window.

## Privacy Features

- Content protection enabled in alignment mode
- No screen content is shared until you explicitly switch to sharing mode
- Easy toggling between modes with a global hotkey

## Building from Source

1. Make sure you have Rust installed. If not, get it from [rustup.rs](https://rustup.rs)
2. Clone this repository
3. Build and run with:
   ```
   cargo build --release
   cargo run --release
   ```

The release build will be available in `target/release/screen-region-share`