//! commands/media.rs — Run remote FFmpeg encodes.

use sftpapp_engine::media::EncodeOptions;
use tauri::State;
use uuid::Uuid;

use crate::commands::CmdResult;
use crate::state::AppState;

/// Clip and transcode a remote video on its SSH host.
#[tauri::command]
pub async fn encode_video(
    state: State<'_, AppState>,
    session_id: String,
    input_path: String,
    output_path: String,
    start_time: String,
    end_time: Option<String>,
    burn_subtitles: bool,
) -> CmdResult<()> {
    let session_id = Uuid::parse_str(&session_id).map_err(|error| error.to_string())?;
    state
        .engine
        .encode_video(
            session_id,
            EncodeOptions {
                input_path,
                output_path,
                start_time,
                end_time,
                burn_subtitles,
            },
        )
        .await
        .map_err(|error| error.to_string())
}
