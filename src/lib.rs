use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

const APP_IDENTIFIER: &[u8; 8] = b"SNDGIF01";
const AUTH_CODE: &[u8; 3] = b"001";
const PAYLOAD_MAGIC: &[u8; 4] = b"SGA1";
const FORMAT_VERSION: u8 = 1;
const FLAG_LOOP: u8 = 1;
const PAYLOAD_HEADER_LEN: usize = 26;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SoundPayload {
    loop_audio: bool,
    start_ms: u32,
    mime_type: String,
    file_name: String,
    audio: Vec<u8>,
    checksum: u32,
}

#[derive(Debug, Clone, Copy)]
struct ExtensionRange {
    start: usize,
    end: usize,
}

#[derive(Debug)]
struct GifLayout {
    trailer: usize,
    sound_extensions: Vec<ExtensionRange>,
}

#[derive(Debug, Clone)]
pub struct ConversionOptions {
    pub ffmpeg: Option<PathBuf>,
    pub fps: u32,
    pub width: u32,
    pub palette_colors: u16,
    pub audio_bitrate: String,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            ffmpeg: None,
            fps: 15,
            width: 640,
            palette_colors: 128,
            audio_bitrate: "64k".to_owned(),
        }
    }
}

pub fn run_cli() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };

    match command {
        "embed" | "encode" => embed_command(&args[1..]),
        "extract" => extract_command(&args[1..]),
        "inspect" | "info" => inspect_command(&args[1..]),
        "strip" => strip_command(&args[1..]),
        "from-video" | "convert" => convert_command(&args[1..]),
        "ui" | "view" => ui_command(&args[1..]),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        "--version" | "-V" | "version" => {
            println!("soundgif {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        other => Err(format!(
            "unknown command '{other}'\n\nRun 'soundgif help' for usage."
        )),
    }
}

fn print_help() {
    println!(
        "soundgif {version}\n\
         Embed audio inside a GIF while preserving normal GIF playback.\n\n\
         USAGE:\n\
           soundgif embed <input.gif> <audio> -o <output.gif> [OPTIONS]\n\
           soundgif extract <input.gif> -o <audio-output>\n\
           soundgif inspect <input.gif>\n\
           soundgif strip <input.gif> -o <output.gif>\n\
           soundgif from-video <input.mp4> -o <output.gif> [OPTIONS]\n\
           soundgif ui\n\n\
         EMBED OPTIONS:\n\
           -o, --output <PATH>    Output GIF (required)\n\
           --mime <TYPE>         Override the detected audio MIME type\n\
           --start-ms <NUMBER>   Audio start offset in milliseconds (default: 0)\n\
           --no-loop             Do not loop audio with the GIF\n\n\
         VIDEO OPTIONS:\n\
           --fps <NUMBER>        GIF frames per second (default: 15)\n\
           --width <PIXELS>      Maximum GIF width (default: 640)\n\
           --colors <NUMBER>     GIF palette colors, 2-256 (default: 128)\n\
           --audio-bitrate <N>   Opus fallback bitrate (default: 64k)\n\
           --ffmpeg <PATH>       Path to ffmpeg executable\n\n\
         Existing SoundGIF data is replaced when embedding again.",
        version = env!("CARGO_PKG_VERSION")
    );
}

fn convert_command(args: &[String]) -> Result<()> {
    let mut input = None;
    let mut output = None;
    let mut options = ConversionOptions::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                output = Some(PathBuf::from(option_value(args, &mut i, "output")?))
            }
            "--ffmpeg" => {
                options.ffmpeg = Some(PathBuf::from(option_value(args, &mut i, "ffmpeg")?));
            }
            "--fps" => {
                let value = option_value(args, &mut i, "fps")?;
                options.fps = value
                    .parse()
                    .map_err(|_| format!("invalid --fps value '{value}'"))?;
            }
            "--width" => {
                let value = option_value(args, &mut i, "width")?;
                options.width = value
                    .parse()
                    .map_err(|_| format!("invalid --width value '{value}'"))?;
            }
            "--colors" => {
                let value = option_value(args, &mut i, "colors")?;
                options.palette_colors = value
                    .parse()
                    .map_err(|_| format!("invalid --colors value '{value}'"))?;
            }
            "--audio-bitrate" => {
                options.audio_bitrate = option_value(args, &mut i, "audio-bitrate")?
            }
            option if option.starts_with('-') => return Err(format!("unknown option '{option}'")),
            value if input.is_none() => input = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument '{value}'")),
        }
        i += 1;
    }

    let input = input.ok_or_else(|| "from-video requires <input-video>".to_owned())?;
    let output = output.ok_or_else(|| "from-video requires -o <output.gif>".to_owned())?;
    ensure_distinct_output(&input, &output)?;
    convert_video_to_soundgif(&input, &output, &options)?;
    println!("Created SoundGIF '{}'.", output.display());
    Ok(())
}

pub fn convert_video_to_soundgif(
    input: &Path,
    output: &Path,
    options: &ConversionOptions,
) -> Result<()> {
    if !input.is_file() {
        return Err(format!("input video '{}' does not exist", input.display()));
    }
    if !(1..=60).contains(&options.fps) {
        return Err("video FPS must be between 1 and 60".to_owned());
    }
    if !(32..=4096).contains(&options.width) {
        return Err("GIF width must be between 32 and 4096 pixels".to_owned());
    }
    if !(2..=256).contains(&options.palette_colors) {
        return Err("GIF palette must contain between 2 and 256 colors".to_owned());
    }
    if options.audio_bitrate.is_empty()
        || !options.audio_bitrate.chars().all(|character| {
            character.is_ascii_digit() || matches!(character, 'k' | 'K' | 'm' | 'M')
        })
    {
        return Err("audio bitrate must look like '96k' or '1M'".to_owned());
    }

    let ffmpeg = resolve_ffmpeg(options.ffmpeg.as_deref())?;
    let temp = ConversionTemp::new()?;
    let filter = gif_filter(options);

    run_ffmpeg(
        &ffmpeg,
        &[
            "-hide_banner".as_ref(),
            "-loglevel".as_ref(),
            "error".as_ref(),
            "-y".as_ref(),
            "-i".as_ref(),
            input.as_os_str(),
            "-filter_complex".as_ref(),
            filter.as_ref(),
            "-gifflags".as_ref(),
            "+offsetting+transdiff".as_ref(),
            "-loop".as_ref(),
            "0".as_ref(),
            temp.gif.as_os_str(),
        ],
        "rendering GIF frames",
    )?;
    let converted_audio = convert_audio(&ffmpeg, input, &temp, &options.audio_bitrate)?;

    let gif = read_file(&temp.gif)?;
    let audio = read_file(&converted_audio.path)?;
    let layout = parse_gif(&gif)?;
    let source_name = input
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("video");
    let payload = SoundPayload {
        loop_audio: true,
        start_ms: 0,
        mime_type: converted_audio.mime_type.to_owned(),
        file_name: format!("{source_name}.{}", converted_audio.extension),
        checksum: crc32(&audio),
        audio,
    };
    let output_bytes = replace_sound_extension(&gif, &layout, Some(&encode_extension(&payload)?));
    write_file(output, &output_bytes)
}

fn gif_filter(options: &ConversionOptions) -> String {
    format!(
        "[0:v]fps={},scale=w='min({},iw)':h=-2:flags=lanczos,split[v1][v2];[v1]palettegen=stats_mode=diff:max_colors={}:reserve_transparent=0[p];[v2][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
        options.fps, options.width, options.palette_colors
    )
}

struct ConvertedAudio {
    path: PathBuf,
    mime_type: &'static str,
    extension: &'static str,
}

fn convert_audio(
    ffmpeg: &Path,
    input: &Path,
    temp: &ConversionTemp,
    fallback_bitrate: &str,
) -> Result<ConvertedAudio> {
    let copy_target = match input
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp4" | "m4v" | "mov") => Some((&temp.audio_m4a, "audio/mp4", "m4a")),
        Some("webm") => Some((&temp.audio_webm, "audio/webm", "webm")),
        _ => None,
    };

    if let Some((path, mime_type, extension)) = copy_target
        && run_ffmpeg(
            ffmpeg,
            &[
                "-hide_banner".as_ref(),
                "-loglevel".as_ref(),
                "error".as_ref(),
                "-y".as_ref(),
                "-i".as_ref(),
                input.as_os_str(),
                "-map".as_ref(),
                "0:a:0".as_ref(),
                "-vn".as_ref(),
                "-c:a".as_ref(),
                "copy".as_ref(),
                path.as_os_str(),
            ],
            "copying the original audio",
        )
        .is_ok()
    {
        return Ok(ConvertedAudio {
            path: path.clone(),
            mime_type,
            extension,
        });
    }

    run_ffmpeg(
        ffmpeg,
        &[
            "-hide_banner".as_ref(),
            "-loglevel".as_ref(),
            "error".as_ref(),
            "-y".as_ref(),
            "-i".as_ref(),
            input.as_os_str(),
            "-map".as_ref(),
            "0:a:0".as_ref(),
            "-vn".as_ref(),
            "-c:a".as_ref(),
            "libopus".as_ref(),
            "-b:a".as_ref(),
            fallback_bitrate.as_ref(),
            "-vbr".as_ref(),
            "on".as_ref(),
            "-compression_level".as_ref(),
            "10".as_ref(),
            "-frame_duration".as_ref(),
            "60".as_ref(),
            temp.audio_opus.as_os_str(),
        ],
        "extracting audio (the video must contain an audio track)",
    )?;
    Ok(ConvertedAudio {
        path: temp.audio_opus.clone(),
        mime_type: "audio/opus",
        extension: "opus",
    })
}

fn resolve_ffmpeg(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return if path.is_file() {
            Ok(path.to_path_buf())
        } else {
            Err(format!("ffmpeg was not found at '{}'", path.display()))
        };
    }
    if let Ok(executable) = env::current_exe() {
        let sibling = executable.with_file_name(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        });
        if sibling.is_file() {
            return Ok(sibling);
        }
    }
    let command = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    match background_command(Path::new(command))
        .arg("-version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
    {
        Ok(status) if status.success() => Ok(PathBuf::from(command)),
        _ => Err(format!(
            "FFmpeg is required for video conversion. Put {command} beside SoundGIF Player or add ffmpeg to PATH."
        )),
    }
}

fn run_ffmpeg(binary: &Path, args: &[&std::ffi::OsStr], stage: &str) -> Result<()> {
    let result = background_command(binary)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run FFmpeg while {stage}: {error}"))?;
    if result.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&result.stderr).trim().to_owned();
    Err(if stderr.is_empty() {
        format!("FFmpeg failed while {stage}")
    } else {
        format!("FFmpeg failed while {stage}: {stderr}")
    })
}

fn background_command(program: &Path) -> Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = Command::new(program);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new(program)
    }
}

struct ConversionTemp {
    directory: PathBuf,
    gif: PathBuf,
    audio_opus: PathBuf,
    audio_m4a: PathBuf,
    audio_webm: PathBuf,
}

impl ConversionTemp {
    fn new() -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock error: {error}"))?
            .as_nanos();
        let directory = env::temp_dir().join(format!("soundgif-{}-{nonce}", process::id()));
        fs::create_dir(&directory)
            .map_err(|error| format!("cannot create temporary conversion directory: {error}"))?;
        Ok(Self {
            gif: directory.join("frames.gif"),
            audio_opus: directory.join("audio.opus"),
            audio_m4a: directory.join("audio.m4a"),
            audio_webm: directory.join("audio.webm"),
            directory,
        })
    }
}

impl Drop for ConversionTemp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.gif);
        let _ = fs::remove_file(&self.audio_opus);
        let _ = fs::remove_file(&self.audio_m4a);
        let _ = fs::remove_file(&self.audio_webm);
        let _ = fs::remove_dir(&self.directory);
    }
}

fn ui_command(args: &[String]) -> Result<()> {
    if !args.is_empty() {
        return Err("ui does not accept any options".to_owned());
    }
    let executable = env::current_exe()
        .map_err(|error| format!("cannot locate the current executable: {error}"))?;
    let player_name = if cfg!(windows) {
        "soundgif-player.exe"
    } else {
        "soundgif-player"
    };
    let player = executable.with_file_name(player_name);
    Command::new(&player).spawn().map(|_| ()).map_err(|error| {
        format!(
            "cannot launch '{}': {error}. Build both executables with 'cargo build --release --bins'.",
            player.display()
        )
    })
}

fn embed_command(args: &[String]) -> Result<()> {
    let mut positional = Vec::new();
    let mut output = None;
    let mut mime_override = None;
    let mut start_ms = 0_u32;
    let mut loop_audio = true;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => output = Some(option_value(args, &mut i, "output")?),
            "--mime" => mime_override = Some(option_value(args, &mut i, "mime")?),
            "--start-ms" => {
                let value = option_value(args, &mut i, "start-ms")?;
                start_ms = value
                    .parse()
                    .map_err(|_| format!("invalid --start-ms value '{value}'"))?;
            }
            "--no-loop" => loop_audio = false,
            option if option.starts_with('-') => return Err(format!("unknown option '{option}'")),
            value => positional.push(value.to_owned()),
        }
        i += 1;
    }

    if positional.len() != 2 {
        return Err("embed requires <input.gif> and <audio>".to_owned());
    }
    let output = output.ok_or_else(|| "embed requires -o <output.gif>".to_owned())?;
    let gif_path = Path::new(&positional[0]);
    let audio_path = Path::new(&positional[1]);
    ensure_distinct_output(gif_path, Path::new(&output))?;

    let gif = read_file(gif_path)?;
    let audio = read_file(audio_path)?;
    let layout = parse_gif(&gif)?;
    let mime_type = mime_override.unwrap_or_else(|| detect_mime(audio_path).to_owned());
    let file_name = audio_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("audio.bin")
        .to_owned();
    let checksum = crc32(&audio);
    let payload = SoundPayload {
        loop_audio,
        start_ms,
        mime_type,
        file_name,
        audio,
        checksum,
    };

    let output_bytes = replace_sound_extension(&gif, &layout, Some(&encode_extension(&payload)?));
    write_file(Path::new(&output), &output_bytes)?;
    println!(
        "Embedded {} bytes of {} audio in '{}' (CRC-32 {:08x}).",
        payload.audio.len(),
        payload.mime_type,
        output,
        payload.checksum
    );
    Ok(())
}

fn extract_command(args: &[String]) -> Result<()> {
    let (input, output) = parse_input_output(args, "extract")?;
    let gif = read_file(&input)?;
    let payload = find_payload(&gif)?;
    verify_checksum(&payload)?;
    write_file(&output, &payload.audio)?;
    println!(
        "Extracted {} bytes to '{}' ({}).",
        payload.audio.len(),
        output.display(),
        payload.mime_type
    );
    Ok(())
}

fn inspect_command(args: &[String]) -> Result<()> {
    if args.len() != 1 {
        return Err("inspect requires exactly one <input.gif>".to_owned());
    }
    let gif = read_file(Path::new(&args[0]))?;
    let layout = parse_gif(&gif)?;
    if layout.sound_extensions.is_empty() {
        println!("No SoundGIF audio found in '{}'.", args[0]);
        return Ok(());
    }
    if layout.sound_extensions.len() > 1 {
        println!(
            "Warning: found {} SoundGIF extensions; showing the first.",
            layout.sound_extensions.len()
        );
    }
    let payload = decode_extension(&gif, layout.sound_extensions[0])?;
    let actual_checksum = crc32(&payload.audio);
    println!("SoundGIF version: {FORMAT_VERSION}");
    println!("Audio file:       {}", payload.file_name);
    println!("MIME type:        {}", payload.mime_type);
    println!("Audio bytes:      {}", payload.audio.len());
    println!("Start offset:     {} ms", payload.start_ms);
    println!(
        "Loop:             {}",
        if payload.loop_audio { "yes" } else { "no" }
    );
    println!("CRC-32:           {:08x}", payload.checksum);
    println!(
        "Integrity:        {}",
        if actual_checksum == payload.checksum {
            "OK"
        } else {
            "FAILED"
        }
    );
    if actual_checksum != payload.checksum {
        return Err(format!(
            "audio checksum mismatch: expected {:08x}, got {:08x}",
            payload.checksum, actual_checksum
        ));
    }
    Ok(())
}

fn strip_command(args: &[String]) -> Result<()> {
    let (input, output) = parse_input_output(args, "strip")?;
    ensure_distinct_output(&input, &output)?;
    let gif = read_file(&input)?;
    let layout = parse_gif(&gif)?;
    if layout.sound_extensions.is_empty() {
        return Err(format!("no SoundGIF audio found in '{}'", input.display()));
    }
    let count = layout.sound_extensions.len();
    let output_bytes = replace_sound_extension(&gif, &layout, None);
    write_file(&output, &output_bytes)?;
    println!(
        "Removed {count} SoundGIF extension(s) into '{}'.",
        output.display()
    );
    Ok(())
}

fn option_value(args: &[String], index: &mut usize, name: &str) -> Result<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("--{name} requires a value"))
}

fn parse_input_output(args: &[String], command: &str) -> Result<(PathBuf, PathBuf)> {
    let mut input = None;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                output = Some(PathBuf::from(option_value(args, &mut i, "output")?))
            }
            option if option.starts_with('-') => return Err(format!("unknown option '{option}'")),
            value if input.is_none() => input = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument '{value}'")),
        }
        i += 1;
    }
    Ok((
        input.ok_or_else(|| format!("{command} requires <input.gif>"))?,
        output.ok_or_else(|| format!("{command} requires -o <output>"))?,
    ))
}

fn ensure_distinct_output(input: &Path, output: &Path) -> Result<()> {
    let input_absolute = absolute_path(input)?;
    let output_absolute = absolute_path(output)?;
    if input_absolute == output_absolute {
        return Err("input and output paths must be different".to_owned());
    }
    Ok(())
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| format!("cannot resolve '{}': {error}", path.display()))
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| format!("cannot read '{}': {error}", path.display()))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).map_err(|error| format!("cannot write '{}': {error}", path.display()))
}

fn detect_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("opus") => "audio/opus",
        Some("ogg") | Some("oga") => "audio/ogg",
        Some("mp3") => "audio/mpeg",
        Some("wav") | Some("wave") => "audio/wav",
        Some("flac") => "audio/flac",
        Some("aac") => "audio/aac",
        Some("m4a") | Some("mp4") => "audio/mp4",
        Some("webm") => "audio/webm",
        _ => "application/octet-stream",
    }
}

fn parse_gif(bytes: &[u8]) -> Result<GifLayout> {
    if bytes.len() < 14 || (&bytes[..6] != b"GIF87a" && &bytes[..6] != b"GIF89a") {
        return Err("input is not a valid GIF87a/GIF89a file".to_owned());
    }

    let packed = bytes[10];
    let global_color_table_len = if packed & 0x80 != 0 {
        3_usize << ((packed & 0x07) as usize + 1)
    } else {
        0
    };
    let mut cursor = 13_usize
        .checked_add(global_color_table_len)
        .ok_or_else(|| "GIF size overflow".to_owned())?;
    require_len(bytes, cursor, 1, "global color table")?;
    let mut sound_extensions = Vec::new();

    loop {
        require_len(bytes, cursor, 1, "GIF block")?;
        match bytes[cursor] {
            0x3b => {
                return Ok(GifLayout {
                    trailer: cursor,
                    sound_extensions,
                });
            }
            0x2c => {
                require_len(bytes, cursor, 10, "image descriptor")?;
                let image_packed = bytes[cursor + 9];
                cursor += 10;
                if image_packed & 0x80 != 0 {
                    let table_len = 3_usize << ((image_packed & 0x07) as usize + 1);
                    require_len(bytes, cursor, table_len, "local color table")?;
                    cursor += table_len;
                }
                require_len(bytes, cursor, 1, "LZW minimum code size")?;
                cursor += 1;
                cursor = skip_sub_blocks(bytes, cursor)?;
            }
            0x21 => {
                let start = cursor;
                require_len(bytes, cursor, 2, "extension introducer")?;
                let label = bytes[cursor + 1];
                cursor += 2;
                let is_sound = if label == 0xff {
                    require_len(bytes, cursor, 1, "application extension size")?;
                    let header_len = bytes[cursor] as usize;
                    require_len(
                        bytes,
                        cursor + 1,
                        header_len,
                        "application extension header",
                    )?;
                    let header = &bytes[cursor + 1..cursor + 1 + header_len];
                    cursor += 1 + header_len;
                    header_len == 11 && &header[..8] == APP_IDENTIFIER && &header[8..] == AUTH_CODE
                } else {
                    false
                };
                cursor = skip_sub_blocks(bytes, cursor)?;
                if is_sound {
                    sound_extensions.push(ExtensionRange { start, end: cursor });
                }
            }
            marker => {
                return Err(format!(
                    "invalid GIF block marker 0x{marker:02x} at byte {cursor}"
                ));
            }
        }
    }
}

fn skip_sub_blocks(bytes: &[u8], mut cursor: usize) -> Result<usize> {
    loop {
        require_len(bytes, cursor, 1, "data sub-block size")?;
        let len = bytes[cursor] as usize;
        cursor += 1;
        if len == 0 {
            return Ok(cursor);
        }
        require_len(bytes, cursor, len, "data sub-block")?;
        cursor += len;
    }
}

fn require_len(bytes: &[u8], start: usize, len: usize, context: &str) -> Result<()> {
    if start.checked_add(len).is_none_or(|end| end > bytes.len()) {
        Err(format!(
            "truncated GIF while reading {context} at byte {start}"
        ))
    } else {
        Ok(())
    }
}

fn encode_extension(payload: &SoundPayload) -> Result<Vec<u8>> {
    let mime = payload.mime_type.as_bytes();
    let name = payload.file_name.as_bytes();
    let mime_len: u16 = mime
        .len()
        .try_into()
        .map_err(|_| "MIME type is too long".to_owned())?;
    let name_len: u16 = name
        .len()
        .try_into()
        .map_err(|_| "audio filename is too long".to_owned())?;
    let audio_len: u64 = payload
        .audio
        .len()
        .try_into()
        .map_err(|_| "audio is too large".to_owned())?;

    let capacity = PAYLOAD_HEADER_LEN
        .checked_add(mime.len())
        .and_then(|value| value.checked_add(name.len()))
        .and_then(|value| value.checked_add(payload.audio.len()))
        .ok_or_else(|| "payload size overflow".to_owned())?;
    let mut data = Vec::with_capacity(capacity);
    data.extend_from_slice(PAYLOAD_MAGIC);
    data.push(FORMAT_VERSION);
    data.push(if payload.loop_audio { FLAG_LOOP } else { 0 });
    data.extend_from_slice(&payload.start_ms.to_le_bytes());
    data.extend_from_slice(&audio_len.to_le_bytes());
    data.extend_from_slice(&payload.checksum.to_le_bytes());
    data.extend_from_slice(&mime_len.to_le_bytes());
    data.extend_from_slice(&name_len.to_le_bytes());
    data.extend_from_slice(mime);
    data.extend_from_slice(name);
    data.extend_from_slice(&payload.audio);

    let block_count = data.len().div_ceil(255);
    let mut extension = Vec::with_capacity(15 + data.len() + block_count);
    extension.extend_from_slice(&[0x21, 0xff, 0x0b]);
    extension.extend_from_slice(APP_IDENTIFIER);
    extension.extend_from_slice(AUTH_CODE);
    for chunk in data.chunks(255) {
        extension.push(chunk.len() as u8);
        extension.extend_from_slice(chunk);
    }
    extension.push(0);
    Ok(extension)
}

fn decode_extension(bytes: &[u8], range: ExtensionRange) -> Result<SoundPayload> {
    let mut cursor = range.start + 14;
    let mut data = Vec::new();
    while cursor < range.end {
        let len = bytes[cursor] as usize;
        cursor += 1;
        if len == 0 {
            break;
        }
        data.extend_from_slice(&bytes[cursor..cursor + len]);
        cursor += len;
    }
    decode_payload(&data)
}

fn decode_payload(data: &[u8]) -> Result<SoundPayload> {
    if data.len() < PAYLOAD_HEADER_LEN {
        return Err("SoundGIF payload is truncated".to_owned());
    }
    if &data[..4] != PAYLOAD_MAGIC {
        return Err("SoundGIF payload has an invalid magic value".to_owned());
    }
    if data[4] != FORMAT_VERSION {
        return Err(format!("unsupported SoundGIF payload version {}", data[4]));
    }
    let flags = data[5];
    let start_ms = read_u32(data, 6)?;
    let audio_len_u64 = read_u64(data, 10)?;
    let audio_len: usize = audio_len_u64
        .try_into()
        .map_err(|_| "embedded audio is too large for this platform".to_owned())?;
    let checksum = read_u32(data, 18)?;
    let mime_len = read_u16(data, 22)? as usize;
    let name_len = read_u16(data, 24)? as usize;
    let metadata_end = PAYLOAD_HEADER_LEN
        .checked_add(mime_len)
        .and_then(|value| value.checked_add(name_len))
        .ok_or_else(|| "SoundGIF metadata size overflow".to_owned())?;
    let payload_end = metadata_end
        .checked_add(audio_len)
        .ok_or_else(|| "SoundGIF payload size overflow".to_owned())?;
    if payload_end != data.len() {
        return Err(format!(
            "SoundGIF payload length mismatch: header describes {payload_end} bytes, found {}",
            data.len()
        ));
    }
    let mime_type = std::str::from_utf8(&data[PAYLOAD_HEADER_LEN..PAYLOAD_HEADER_LEN + mime_len])
        .map_err(|_| "SoundGIF MIME type is not valid UTF-8".to_owned())?
        .to_owned();
    let file_name = std::str::from_utf8(&data[PAYLOAD_HEADER_LEN + mime_len..metadata_end])
        .map_err(|_| "SoundGIF filename is not valid UTF-8".to_owned())?
        .to_owned();
    Ok(SoundPayload {
        loop_audio: flags & FLAG_LOOP != 0,
        start_ms,
        mime_type,
        file_name,
        audio: data[metadata_end..payload_end].to_vec(),
        checksum,
    })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let value = data
        .get(offset..offset + 2)
        .ok_or_else(|| "SoundGIF header is truncated".to_owned())?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let value = data
        .get(offset..offset + 4)
        .ok_or_else(|| "SoundGIF header is truncated".to_owned())?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64> {
    let value = data
        .get(offset..offset + 8)
        .ok_or_else(|| "SoundGIF header is truncated".to_owned())?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

fn find_payload(bytes: &[u8]) -> Result<SoundPayload> {
    let layout = parse_gif(bytes)?;
    let range = layout
        .sound_extensions
        .first()
        .copied()
        .ok_or_else(|| "no SoundGIF audio found".to_owned())?;
    decode_extension(bytes, range)
}

fn verify_checksum(payload: &SoundPayload) -> Result<()> {
    let actual = crc32(&payload.audio);
    if actual != payload.checksum {
        Err(format!(
            "audio checksum mismatch: expected {:08x}, got {:08x}",
            payload.checksum, actual
        ))
    } else {
        Ok(())
    }
}

fn replace_sound_extension(
    bytes: &[u8],
    layout: &GifLayout,
    replacement: Option<&[u8]>,
) -> Vec<u8> {
    let removed_len: usize = layout
        .sound_extensions
        .iter()
        .map(|range| range.end - range.start)
        .sum();
    let replacement_len = replacement.map_or(0, <[u8]>::len);
    let mut output = Vec::with_capacity(bytes.len() - removed_len + replacement_len);
    let mut cursor = 0;
    for range in &layout.sound_extensions {
        output.extend_from_slice(&bytes[cursor..range.start]);
        cursor = range.end;
    }
    output.extend_from_slice(&bytes[cursor..layout.trailer]);
    if let Some(extension) = replacement {
        output.extend_from_slice(extension);
    }
    output.extend_from_slice(&bytes[layout.trailer..]);
    output
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    // A complete, valid 1x1 GIF89a with a two-entry global color table.
    const TINY_GIF: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xff, 0xff, 0xff, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02,
        0x02, 0x44, 0x01, 0x00, 0x3b,
    ];

    fn sample_payload(size: usize) -> SoundPayload {
        let audio: Vec<u8> = (0..size).map(|value| (value % 251) as u8).collect();
        SoundPayload {
            loop_audio: true,
            start_ms: 125,
            mime_type: "audio/opus".to_owned(),
            file_name: "sample.opus".to_owned(),
            checksum: crc32(&audio),
            audio,
        }
    }

    #[test]
    fn parses_tiny_gif() {
        let layout = parse_gif(TINY_GIF).unwrap();
        assert_eq!(layout.trailer, TINY_GIF.len() - 1);
        assert!(layout.sound_extensions.is_empty());
    }

    #[test]
    fn round_trips_payload_across_many_sub_blocks() {
        let payload = sample_payload(4096);
        let extension = encode_extension(&payload).unwrap();
        let layout = parse_gif(TINY_GIF).unwrap();
        let encoded = replace_sound_extension(TINY_GIF, &layout, Some(&extension));
        let decoded = find_payload(&encoded).unwrap();
        assert_eq!(decoded, payload);
        assert_eq!(*encoded.last().unwrap(), 0x3b);
    }

    #[test]
    fn replacing_audio_does_not_duplicate_extension() {
        let first = sample_payload(300);
        let second = sample_payload(700);
        let layout = parse_gif(TINY_GIF).unwrap();
        let once =
            replace_sound_extension(TINY_GIF, &layout, Some(&encode_extension(&first).unwrap()));
        let once_layout = parse_gif(&once).unwrap();
        let twice = replace_sound_extension(
            &once,
            &once_layout,
            Some(&encode_extension(&second).unwrap()),
        );
        let twice_layout = parse_gif(&twice).unwrap();
        assert_eq!(twice_layout.sound_extensions.len(), 1);
        assert_eq!(find_payload(&twice).unwrap(), second);
    }

    #[test]
    fn stripping_restores_original_gif() {
        let layout = parse_gif(TINY_GIF).unwrap();
        let encoded = replace_sound_extension(
            TINY_GIF,
            &layout,
            Some(&encode_extension(&sample_payload(512)).unwrap()),
        );
        let encoded_layout = parse_gif(&encoded).unwrap();
        let stripped = replace_sound_extension(&encoded, &encoded_layout, None);
        assert_eq!(stripped, TINY_GIF);
    }

    #[test]
    fn detects_corrupted_audio() {
        let layout = parse_gif(TINY_GIF).unwrap();
        let extension = encode_extension(&sample_payload(64)).unwrap();
        let mut encoded = replace_sound_extension(TINY_GIF, &layout, Some(&extension));
        let last_audio_byte = encoded.len() - 3;
        encoded[last_audio_byte] ^= 0xff;
        let payload = find_payload(&encoded).unwrap();
        assert!(verify_checksum(&payload).is_err());
    }

    #[test]
    fn crc32_matches_standard_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn video_conversion_rejects_missing_input_before_running_ffmpeg() {
        let missing = env::temp_dir().join("soundgif-test-input-that-does-not-exist.mp4");
        let output = env::temp_dir().join("soundgif-test-output.gif");
        let error = convert_video_to_soundgif(&missing, &output, &ConversionOptions::default())
            .unwrap_err();
        assert!(error.contains("does not exist"));
    }

    #[test]
    fn explicit_missing_ffmpeg_has_a_clear_error() {
        let missing = env::temp_dir().join("soundgif-test-ffmpeg-that-does-not-exist.exe");
        let error = resolve_ffmpeg(Some(&missing)).unwrap_err();
        assert!(error.contains("ffmpeg was not found"));
    }
}
