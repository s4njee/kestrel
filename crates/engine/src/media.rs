//! media.rs — Remote media encoding through an isolated SSH exec channel.
//!
//! FFmpeg runs entirely on the remote host: media bytes never cross the SFTP
//! connection. User-controlled paths and time values are validated/quoted here
//! rather than interpolated by the webview.

use std::time::Duration;

use uuid::Uuid;

use crate::error::{EngineError, Result};
use crate::exec::shell_quote;
use crate::session::Engine;

/// Long-running encodes get their own timeout rather than the short probe
/// timeout used by checksums and tar detection.
const ENCODE_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

/// Options for one remote FFmpeg encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeOptions {
    pub input_path: String,
    pub output_path: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub burn_subtitles: bool,
}

impl Engine {
    /// Clip and transcode a remote video to x265 CRF 18 with Opus audio.
    ///
    /// The process runs on a separate SSH exec channel and refuses to overwrite
    /// an existing output (`-n`). When subtitles are not burned, all subtitle
    /// streams are copied into the output container. Burning uses the first
    /// embedded subtitle stream and omits separate subtitle tracks.
    pub async fn encode_video(&self, session_id: Uuid, options: EncodeOptions) -> Result<()> {
        let session = self
            .session(session_id)
            .ok_or_else(|| EngineError::NotFound(format!("session {session_id}")))?;
        let command = ffmpeg_command(&options)?;
        let output = session.exec(&command, ENCODE_TIMEOUT).await?;
        if output.ok() {
            Ok(())
        } else {
            let detail = output.stderr_text();
            let detail = if detail.is_empty() {
                format!("ffmpeg exited with status {:?}", output.exit_status)
            } else {
                detail
            };
            Err(EngineError::Protocol(detail))
        }
    }
}

/// Build a safely quoted FFmpeg command line.
fn ffmpeg_command(options: &EncodeOptions) -> Result<String> {
    if options.input_path.is_empty() || options.output_path.is_empty() {
        return Err(EngineError::InvalidPath(
            "input and output paths are required".into(),
        ));
    }
    if options.input_path == options.output_path {
        return Err(EngineError::InvalidPath(
            "output path must differ from input path".into(),
        ));
    }
    validate_time(&options.start_time)?;
    let start = parse_time(&options.start_time).expect("validated above");
    if let Some(end) = &options.end_time {
        validate_time(end)?;
        if parse_time(end).expect("validated above") <= start {
            return Err(EngineError::Protocol(
                "end time must be after start time".into(),
            ));
        }
    }

    let mut args = vec![
        "ffmpeg".to_string(),
        "-hide_banner".to_string(),
        "-nostdin".to_string(),
        "-n".to_string(),
        "-i".to_string(),
        shell_quote(&options.input_path),
        "-ss".to_string(),
        shell_quote(&options.start_time),
    ];
    if let Some(end) = &options.end_time {
        args.push("-to".to_string());
        args.push(shell_quote(end));
    }
    if options.burn_subtitles {
        let filter = format!(
            "subtitles=filename={}:si=0",
            escape_filter_value(&options.input_path)
        );
        args.extend([
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            shell_quote("0:a?"),
            "-vf".into(),
            shell_quote(&filter),
            "-sn".into(),
        ]);
    } else {
        args.extend([
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            shell_quote("0:a?"),
            "-map".into(),
            shell_quote("0:s?"),
            "-c:s".into(),
            "copy".into(),
        ]);
    }

    args.extend([
        "-c:v".into(),
        "libx265".into(),
        "-crf".into(),
        "18".into(),
        "-c:a".into(),
        "libopus".into(),
        shell_quote(&options.output_path),
    ]);
    Ok(args.join(" "))
}

/// Accept seconds or `HH:MM:SS(.fraction)` while rejecting shell-like input.
fn validate_time(value: &str) -> Result<()> {
    let valid_chars = value
        .chars()
        .all(|character| character.is_ascii_digit() || character == ':' || character == '.');
    let parsed = parse_time(value);
    if !valid_chars || parsed.is_none() {
        return Err(EngineError::Protocol(format!(
            "invalid time {value:?}; use seconds or HH:MM:SS"
        )));
    }
    Ok(())
}

fn parse_time(value: &str) -> Option<f64> {
    if value.is_empty() {
        return None;
    }
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() > 3 || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let numbers: Vec<f64> = parts
        .iter()
        .map(|part| part.parse::<f64>())
        .collect::<std::result::Result<_, _>>()
        .ok()?;
    if numbers
        .iter()
        .any(|number| !number.is_finite() || *number < 0.0)
    {
        return None;
    }
    match numbers.as_slice() {
        [seconds] => Some(*seconds),
        [minutes, seconds] if *seconds < 60.0 => Some(minutes * 60.0 + seconds),
        [hours, minutes, seconds] if *minutes < 60.0 && *seconds < 60.0 => {
            Some(hours * 3600.0 + minutes * 60.0 + seconds)
        }
        _ => None,
    }
}

/// Escape a filename for FFmpeg's filtergraph parser. The resulting filter is
/// subsequently shell-quoted as one argument.
fn escape_filter_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '\'' | ':' | ',' | '[' | ']' | ';') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> EncodeOptions {
        EncodeOptions {
            input_path: "/video/source file.mkv".into(),
            output_path: "/video/out.mkv".into(),
            start_time: "00:01:02.5".into(),
            end_time: Some("00:03:04".into()),
            burn_subtitles: false,
        }
    }

    #[test]
    fn builds_x265_opus_clip_without_overwrite() {
        let command = ffmpeg_command(&options()).unwrap();
        assert!(command.contains(
            "-n -i '/video/source file.mkv' -ss '00:01:02.5' -to '00:03:04'"
        ));
        assert!(command.contains("-c:v libx265 -crf 18 -c:a libopus"));
        assert!(command.contains("-map '0:s?' -c:s copy"));
        assert!(command.ends_with("'/video/out.mkv'"));
    }

    #[test]
    fn burn_in_uses_first_embedded_subtitle_and_drops_tracks() {
        let mut value = options();
        value.input_path = "/video/a:b's.mkv".into();
        value.burn_subtitles = true;
        let command = ffmpeg_command(&value).unwrap();
        assert!(command.contains("subtitles=filename=/video/a\\:b"));
        assert!(command.contains(":si=0"));
        assert!(command.contains(" -sn "));
        assert!(!command.contains("-c:s copy"));
    }

    #[test]
    fn rejects_invalid_times_and_same_output() {
        let mut value = options();
        value.start_time = "0; touch /tmp/x".into();
        assert!(ffmpeg_command(&value).is_err());

        let mut value = options();
        value.output_path = value.input_path.clone();
        assert!(ffmpeg_command(&value).is_err());
    }
}
