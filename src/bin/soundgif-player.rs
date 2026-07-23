#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use soundgif::{ConversionOptions, convert_video_to_soundgif};
use std::path::{Path, PathBuf};

const MAX_UI_SOUNDGIF_BYTES: u64 = 64 * 1024 * 1024;

enum AppEvent {
    Convert(PathBuf),
    Load(PathBuf),
    PickVideo,
    PickGif,
    Save,
    WindowClose,
    WindowDrag,
    WindowMinimize,
    WindowToggleMaximize,
    SetQuality(String),
    DragActive(bool),
    Converted(Result<ReadyFile, String>),
    Loaded(Result<ReadyFile, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiAction {
    PickVideo,
    PickGif,
    Save,
    WindowClose,
    WindowDrag,
    WindowMinimize,
    WindowToggleMaximize,
    QualityCompact,
    QualityBalanced,
    QualityHigh,
}

struct ReadyFile {
    path: PathBuf,
    encoded: String,
    suggested_name: String,
    default_directory: Option<PathBuf>,
    generated: bool,
}

fn main() {
    if let Err(error) = run() {
        show_error(&format!("SoundGIF could not start.\n\n{error}"));
    }
}

fn run() -> Result<(), String> {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::window::WindowBuilder;
    use wry::{DragDropEvent, WebViewBuilder};

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let event_proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("SoundGIF")
        .with_inner_size(LogicalSize::new(980.0, 760.0))
        .with_min_inner_size(LogicalSize::new(680.0, 600.0))
        .with_decorations(false)
        .with_visible(false)
        .build(&event_loop)
        .map_err(|error| format!("cannot create the app window: {error}"))?;

    let drop_proxy = event_proxy.clone();
    let ipc_proxy = event_proxy.clone();
    let webview_builder = WebViewBuilder::new()
        .with_html(include_str!("../ui_v2.html"))
        .with_autoplay(true)
        .with_hotkeys_zoom(false)
        .with_general_autofill_enabled(false)
        .with_ipc_handler(move |request| {
            if let Some(action) = action_from_message(request.body()) {
                let event = match action {
                    UiAction::PickVideo => AppEvent::PickVideo,
                    UiAction::PickGif => AppEvent::PickGif,
                    UiAction::Save => AppEvent::Save,
                    UiAction::WindowClose => AppEvent::WindowClose,
                    UiAction::WindowDrag => AppEvent::WindowDrag,
                    UiAction::WindowMinimize => AppEvent::WindowMinimize,
                    UiAction::WindowToggleMaximize => AppEvent::WindowToggleMaximize,
                    UiAction::QualityCompact => AppEvent::SetQuality("compact".to_owned()),
                    UiAction::QualityBalanced => AppEvent::SetQuality("balanced".to_owned()),
                    UiAction::QualityHigh => AppEvent::SetQuality("high".to_owned()),
                };
                let _ = ipc_proxy.send_event(event);
            }
        })
        .with_drag_drop_handler(move |event| {
            match event {
                DragDropEvent::Enter { paths, .. } => {
                    let supported = paths.iter().any(|path| is_video(path) || is_gif(path));
                    let _ = drop_proxy.send_event(AppEvent::DragActive(supported));
                }
                DragDropEvent::Over { .. } => {}
                DragDropEvent::Drop { paths, .. } => {
                    let _ = drop_proxy.send_event(AppEvent::DragActive(false));
                    if let Some(path) = paths
                        .into_iter()
                        .find(|path| is_video(path) || is_gif(path))
                    {
                        let event = if is_video(&path) {
                            AppEvent::Convert(path)
                        } else {
                            AppEvent::Load(path)
                        };
                        let _ = drop_proxy.send_event(event);
                    }
                }
                DragDropEvent::Leave => {
                    let _ = drop_proxy.send_event(AppEvent::DragActive(false));
                }
                _ => {}
            }
            true
        });
    #[cfg(not(target_os = "linux"))]
    let webview = webview_builder
        .build(&window)
        .map_err(|error| {
            format!(
                "cannot initialize the system webview: {error}\n\nOn Windows, install or repair Microsoft Edge WebView2 Runtime."
            )
        })?;
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;

        webview_builder
            .build_gtk(window.gtk_window())
            .map_err(|error| format!("cannot initialize the WebKitGTK webview: {error}"))?
    };
    window.set_visible(true);

    let mut converting = false;
    let mut quality = "balanced".to_owned();
    let mut current_file: Option<PathBuf> = None;
    let mut current_name = "soundgif.gif".to_owned();
    let mut current_directory: Option<PathBuf> = None;
    let mut generated_temp: Option<PathBuf> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(AppEvent::SetQuality(value)) => quality = value,
            Event::UserEvent(AppEvent::WindowDrag) => {
                let _ = window.drag_window();
            }
            Event::UserEvent(AppEvent::WindowMinimize) => window.set_minimized(true),
            Event::UserEvent(AppEvent::WindowToggleMaximize) => {
                window.set_maximized(!window.is_maximized());
            }
            Event::UserEvent(AppEvent::WindowClose) => {
                if let Some(path) = generated_temp.take() {
                    let _ = std::fs::remove_file(path);
                }
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(AppEvent::DragActive(active)) => {
                let _ = webview.evaluate_script(if active {
                    "nativeDragActive(true);"
                } else {
                    "nativeDragActive(false);"
                });
            }
            Event::UserEvent(AppEvent::PickVideo) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_parent(&window)
                    .set_title("Open a video to convert")
                    .add_filter("Video", &["mp4", "m4v", "mov", "webm", "mkv", "avi"])
                    .pick_file()
                {
                    let _ = event_proxy.send_event(AppEvent::Convert(path));
                } else {
                    let _ = webview.evaluate_script("nativePickerCancelled();");
                }
            }
            Event::UserEvent(AppEvent::PickGif) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_parent(&window)
                    .set_title("Open a SoundGIF")
                    .add_filter("GIF image", &["gif"])
                    .pick_file()
                {
                    let _ = event_proxy.send_event(AppEvent::Load(path));
                } else {
                    let _ = webview.evaluate_script("nativePickerCancelled();");
                }
            }
            Event::UserEvent(AppEvent::Convert(input)) => {
                if converting {
                    notify_error(&webview, "A conversion is already running.");
                    return;
                }
                converting = true;
                let label = js_string(&input.display().to_string());
                let _ = webview.evaluate_script(&format!("nativeConversionStarted({label});"));
                let finished_proxy = event_proxy.clone();
                let options = options_for_quality(&quality);
                std::thread::spawn(move || {
                    let result = (|| {
                        let suggested_name = suggested_output_name(&input);
                        let output = temporary_output_path(&suggested_name);
                        convert_video_to_soundgif(&input, &output, &options)?;
                        ready_file(
                            output,
                            suggested_name,
                            input.parent().map(Path::to_path_buf),
                            true,
                        )
                    })();
                    let _ = finished_proxy.send_event(AppEvent::Converted(result));
                });
            }
            Event::UserEvent(AppEvent::Load(path)) => {
                if converting {
                    notify_error(&webview, "Wait for the current conversion to finish.");
                    return;
                }
                let label = js_string(&path.display().to_string());
                let _ = webview.evaluate_script(&format!("nativeFileLoading({label});"));
                let loaded_proxy = event_proxy.clone();
                std::thread::spawn(move || {
                    let name = path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("soundgif.gif")
                        .to_owned();
                    let directory = path.parent().map(Path::to_path_buf);
                    let result = ready_file(path, name, directory, false);
                    let _ = loaded_proxy.send_event(AppEvent::Loaded(result));
                });
            }
            Event::UserEvent(AppEvent::Converted(result))
            | Event::UserEvent(AppEvent::Loaded(result)) => {
                converting = false;
                match result {
                    Ok(ready) => {
                        if let Some(previous) = generated_temp.take() {
                            let _ = std::fs::remove_file(previous);
                        }
                        if ready.generated {
                            generated_temp = Some(ready.path.clone());
                        }
                        current_file = Some(ready.path);
                        current_name = ready.suggested_name.clone();
                        current_directory = ready.default_directory;
                        let encoded = js_string(&ready.encoded);
                        let name = js_string(&ready.suggested_name);
                        let created = if ready.generated { "true" } else { "false" };
                        let _ = webview.evaluate_script(&format!(
                            "nativeSoundGifReady({encoded}, {name}, {created});"
                        ));
                    }
                    Err(error) => notify_error(&webview, &error),
                }
            }
            Event::UserEvent(AppEvent::Save) => {
                let Some(source) = current_file.as_ref() else {
                    notify_error(&webview, "There is no SoundGIF to save yet.");
                    return;
                };
                let mut dialog = rfd::FileDialog::new()
                    .set_parent(&window)
                    .set_title("Save SoundGIF as")
                    .add_filter("GIF image", &["gif"])
                    .set_file_name(&current_name);
                if let Some(directory) = current_directory.as_ref() {
                    dialog = dialog.set_directory(directory);
                }
                if let Some(destination) = dialog.save_file() {
                    let result = copy_soundgif(source, &destination);
                    match result {
                        Ok(()) => {
                            current_directory = destination.parent().map(Path::to_path_buf);
                            let path = js_string(&destination.display().to_string());
                            let _ =
                                webview.evaluate_script(&format!("nativeSaveComplete({path});"));
                        }
                        Err(error) => notify_error(&webview, &error),
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if let Some(path) = generated_temp.take() {
                    let _ = std::fs::remove_file(path);
                }
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

fn ready_file(
    path: PathBuf,
    suggested_name: String,
    default_directory: Option<PathBuf>,
    generated: bool,
) -> Result<ReadyFile, String> {
    use base64::Engine;
    let file_size = std::fs::metadata(&path)
        .map_err(|error| format!("Could not inspect '{}': {error}", path.display()))?
        .len();
    if file_size == 0 || file_size > MAX_UI_SOUNDGIF_BYTES {
        if generated {
            let _ = std::fs::remove_file(&path);
        }
        return Err(format!(
            "SoundGIF files opened in the desktop player must be between 1 byte and {} MiB.",
            MAX_UI_SOUNDGIF_BYTES / 1024 / 1024
        ));
    }
    let bytes = std::fs::read(&path).map_err(|error| {
        if generated {
            let _ = std::fs::remove_file(&path);
        }
        format!("Could not read '{}': {error}", path.display())
    })?;
    Ok(ReadyFile {
        path,
        encoded: base64::engine::general_purpose::STANDARD.encode(bytes),
        suggested_name,
        default_directory,
        generated,
    })
}

fn options_for_quality(quality: &str) -> ConversionOptions {
    let mut options = ConversionOptions::default();
    match quality {
        "compact" => {
            options.fps = 12;
            options.width = 480;
            options.palette_colors = 96;
            options.audio_bitrate = "48k".to_owned();
        }
        "high" => {
            options.fps = 24;
            options.width = 960;
            options.palette_colors = 192;
            options.audio_bitrate = "96k".to_owned();
        }
        _ => {}
    }
    options
}

fn is_video(path: &Path) -> bool {
    matches!(
        extension(path).as_deref(),
        Some("mp4" | "m4v" | "mov" | "webm" | "mkv" | "avi")
    )
}

fn is_gif(path: &Path) -> bool {
    extension(path).as_deref() == Some("gif")
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
}

fn action_from_message(message: &str) -> Option<UiAction> {
    match message {
        "pick-video" => Some(UiAction::PickVideo),
        "pick-gif" => Some(UiAction::PickGif),
        "save" => Some(UiAction::Save),
        "window:close" => Some(UiAction::WindowClose),
        "window:drag" => Some(UiAction::WindowDrag),
        "window:minimize" => Some(UiAction::WindowMinimize),
        "window:toggle-maximize" => Some(UiAction::WindowToggleMaximize),
        "quality:compact" => Some(UiAction::QualityCompact),
        "quality:balanced" => Some(UiAction::QualityBalanced),
        "quality:high" => Some(UiAction::QualityHigh),
        _ => None,
    }
}

fn suggested_output_name(input: &Path) -> String {
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    format!("{stem}.sound.gif")
}

fn temporary_output_path(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!("soundgif-{}-{nonce}-{name}", std::process::id()))
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn copy_soundgif(source: &Path, destination: &Path) -> Result<(), String> {
    if same_path(source, destination) {
        return Ok(());
    }
    std::fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| format!("Could not save the SoundGIF: {error}"))
}

fn notify_error(webview: &wry::WebView, message: &str) {
    let message = js_string(message);
    let _ = webview.evaluate_script(&format!("nativeOperationFailed({message});"));
}

fn js_string(value: &str) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

#[cfg(target_os = "windows")]
fn show_error(message: &str) {
    use std::ffi::c_void;
    use std::ptr;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            window: *mut c_void,
            text: *const u16,
            caption: *const u16,
            kind: u32,
        ) -> i32;
    }

    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let caption: Vec<u16> = "SoundGIF".encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
    }
}

#[cfg(not(target_os = "windows"))]
fn show_error(message: &str) {
    eprintln!("SoundGIF could not start: {message}");
    let _ = rfd::MessageDialog::new()
        .set_title("SoundGIF")
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_presets_have_distinct_conversion_settings() {
        let compact = options_for_quality("compact");
        let balanced = options_for_quality("balanced");
        let high = options_for_quality("high");
        assert!(compact.width < balanced.width);
        assert!(balanced.width < high.width);
        assert!(compact.fps < high.fps);
    }

    #[test]
    fn save_copy_preserves_the_complete_soundgif() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("soundgif-save-test-{nonce}"));
        std::fs::create_dir(&directory).unwrap();
        let source = directory.join("source.gif");
        let destination = directory.join("saved.gif");
        let bytes = b"GIF89a-soundgif-test-data";
        std::fs::write(&source, bytes).unwrap();
        copy_soundgif(&source, &destination).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        std::fs::remove_file(source).unwrap();
        std::fs::remove_file(destination).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn every_ui_message_routes_to_the_native_host() {
        let routes = [
            ("pick-video", UiAction::PickVideo),
            ("pick-gif", UiAction::PickGif),
            ("save", UiAction::Save),
            ("window:close", UiAction::WindowClose),
            ("window:drag", UiAction::WindowDrag),
            ("window:minimize", UiAction::WindowMinimize),
            ("window:toggle-maximize", UiAction::WindowToggleMaximize),
            ("quality:compact", UiAction::QualityCompact),
            ("quality:balanced", UiAction::QualityBalanced),
            ("quality:high", UiAction::QualityHigh),
        ];
        for (message, expected) in routes {
            assert_eq!(action_from_message(message), Some(expected));
        }
        assert_eq!(action_from_message("unknown"), None);
    }

    #[test]
    fn primary_file_buttons_use_the_ipc_action_bridge() {
        let html = include_str!("../ui_v2.html");
        for action in ["pick-video", "pick-gif", "save"] {
            assert!(
                html.contains(&format!("data-native-action=\"{action}\"")),
                "missing IPC action for {action}"
            );
        }
        assert!(html.contains("window.ipc.postMessage(message)"));
    }
}
