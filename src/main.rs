#[cfg(any(target_os = "macos", target_os = "linux"))]
use bevy::window::CompositeAlphaMode;
use bevy::window::{WindowFocused, WindowMoved, WindowResized};
use bevy::{prelude::*, render::render_resource::Extent3d, window::WindowResolution};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::thread;
use xcap::Monitor;

#[derive(Component)]
struct Screencap;

#[derive(Resource)]
struct FrameReceiver(Receiver<Vec<u8>>);

#[derive(Resource)]
struct WindowSize {
    width: f32,
    height: f32,
}

#[derive(Resource)]
struct ResizeSender(Sender<(u32, u32)>);

#[derive(Resource)]
struct WindowPosition {
    x: u32,
    y: u32,
}

#[derive(Resource)]
struct PositionSender(Sender<(u32, u32)>);

fn main() {
    let (tx, rx) = bounded(1);
    let (resize_tx, resize_rx) = bounded(1);
    let (position_tx, position_rx) = bounded(1);
    let frame_receiver = FrameReceiver(rx);
    let window_size = WindowSize {
        width: 800.0,
        height: 600.0,
    }; // Default size
    let resize_sender = ResizeSender(resize_tx);
    let window_position = WindowPosition { x: 0, y: 0 }; // Default position
    let position_sender = PositionSender(position_tx);

    thread::spawn(move || {
        let monitor = Monitor::from_point(0, 0).unwrap();
        let (video_recorder, sx) = monitor.video_recorder().unwrap();
        video_recorder.start().unwrap();

        let mut current_width = 64;
        let mut current_height = 64;
        let mut current_x = 1;
        let mut current_y = 1;

        loop {
            if let Ok((new_width, new_height)) = resize_rx.try_recv() {
                current_width = new_width;
                current_height = new_height;
            }

            if let Ok((new_x, new_y)) = position_rx.try_recv() {
                current_x = new_x;
                current_y = new_y;
            }

            match sx.recv() {
                Ok(frame) => {
                    let cropped_frame = crop_frame(
                        &frame.raw,
                        frame.width,
                        current_x,
                        current_y,
                        current_width,
                        current_height,
                    );
                    if tx.try_send(cropped_frame).is_err() {
                        continue;
                    }
                }
                _ => continue,
            }
        }
    });

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                // Setting `transparent` allows the `ClearColor`'s alpha value to take effect
                transparent: true,
                // Disabling window decorations to make it feel more like a widget than a window
                decorations: true,
                resolution: WindowResolution::new(800.0, 600.0).with_scale_factor_override(1.0),
                #[cfg(target_os = "macos")]
                composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
                #[cfg(target_os = "linux")]
                composite_alpha_mode: CompositeAlphaMode::PreMultiplied,
                ..default()
            }),
            ..default()
        }))
        // ClearColor must have 0 alpha, otherwise some color will bleed through
        .insert_resource(ClearColor(Color::NONE))
        .insert_resource(frame_receiver)
        .insert_resource(window_size)
        .insert_resource(resize_sender)
        .insert_resource(window_position)
        .insert_resource(position_sender)
        .add_systems(Startup, setup)
        .add_systems(Update, update_sprite_image)
        .add_systems(Update, on_resize_system)
        .add_systems(Update, on_move_system)
        .add_systems(Update, on_focus_system)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((
        Screencap,
        Sprite {
            color: Color::srgba(1.0, 1.0, 1.0, 0.90),
            ..default()
        },
    ));
}

fn on_resize_system(
    mut images: ResMut<Assets<Image>>,
    mut window_size: ResMut<WindowSize>,
    mut resize_reader: EventReader<WindowResized>,
    query: Query<(&Screencap, &mut Sprite)>,
    resize_sender: Res<ResizeSender>,
) {
    for e in resize_reader.read() {
        window_size.width = e.width;
        window_size.height = e.height;
        resize_sender
            .0
            .send((e.width as u32, e.height as u32))
            .unwrap();
    }

    let Ok((_, sprite)) = query.get_single() else {
        return;
    };

    let Some(image) = images.get_mut(&sprite.image) else {
        return;
    };

    let new_size = Extent3d {
        width: window_size.width as u32,
        height: window_size.height as u32,
        ..default()
    };
    image.resize(new_size);
}

fn on_move_system(
    mut window_position: ResMut<WindowPosition>,
    mut move_reader: EventReader<WindowMoved>,
    position_sender: Res<PositionSender>,
) {
    for e in move_reader.read() {
        window_position.x = e.position.x as u32;
        window_position.y = e.position.y as u32;
        position_sender
            .0
            .send((window_position.x, window_position.y))
            .unwrap();
    }
}

fn on_focus_system(
    mut window: Single<&mut Window>,
    mut focus_reader: EventReader<WindowFocused>,
    mut query: Query<&mut Sprite, With<Screencap>>,
) {
    for e in focus_reader.read() {
        if e.focused {
            println!("Focused");
            window.decorations = true;
            if let Ok(mut sprite) = query.get_single_mut() {
                sprite.color.set_alpha(0.5);
            }
        } else {
            println!("Unfocused");
            window.decorations = false;
            if let Ok(mut sprite) = query.get_single_mut() {
                sprite.color.set_alpha(0.9);
            }
        }
    }
}

fn update_sprite_image(
    mut images: ResMut<Assets<Image>>,
    frame_receiver: Res<FrameReceiver>,
    window: Single<&Window>,
    query: Query<(&Screencap, &mut Sprite)>,
) {
    let Ok((_, sprite)) = query.get_single() else {
        return;
    };

    let Some(image) = images.get_mut(&sprite.image) else {
        return;
    };

    if let Ok(new_frame) = frame_receiver.0.try_recv() {
        if !window.focused {
            image.data = new_frame;
        } else {
            let buff = vec![128; image.data.len() * 4];
            image.data = buff;
        }
    }
}

fn crop_frame(
    frame: &[u8],
    original_width: u32,
    upper_left_x: u32,
    upper_left_y: u32,
    new_width: u32,
    new_height: u32,
) -> Vec<u8> {
    let mut new_frame = vec![0; (new_width * new_height * 4) as usize];

    // Check for out-of-bounds access
    if frame.len() < new_frame.len() + (upper_left_y * original_width + upper_left_x) as usize * 4 {
        return new_frame;
    }

    // Copy whole rows at a time for efficiency
    for y in 0..new_height {
        let original_y = upper_left_y + y;
        let original_index = (original_y * original_width + upper_left_x) as usize * 4;
        let new_index = (y * new_width) as usize * 4;
        new_frame[new_index..new_index + (new_width * 4) as usize]
            .copy_from_slice(&frame[original_index..original_index + (new_width * 4) as usize]);
    }
    new_frame
}
