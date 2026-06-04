use download_core::hls::remux::{RemuxInputs, Remuxer};
use download_core::CoreError;
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

/// Remuxer de production : appelle le binaire ffmpeg embarqué (sidecar Tauri).
/// Ne fait que le remux conteneur (`-c copy`) — la concaténation est faite côté cœur.
pub struct SidecarRemuxer {
    app: AppHandle,
}

impl SidecarRemuxer {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl Remuxer for SidecarRemuxer {
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
        let mut args: Vec<String> = vec!["-y".into()];
        args.push("-i".into());
        args.push(inputs.video.to_string_lossy().to_string());
        if let Some(audio) = &inputs.audio {
            args.push("-i".into());
            args.push(audio.to_string_lossy().to_string());
        }
        args.extend(["-c".into(), "copy".into(), "-movflags".into(), "+faststart".into()]);
        args.push(output.to_string_lossy().to_string());

        let sidecar = self
            .app
            .shell()
            .sidecar("ffmpeg")
            .map_err(|e| CoreError::Media(format!("ffmpeg introuvable: {e}")))?;
        let output_run = sidecar
            .args(args)
            .output()
            .await
            .map_err(|e| CoreError::Media(format!("échec exécution ffmpeg: {e}")))?;
        if !output_run.status.success() {
            let stderr = String::from_utf8_lossy(&output_run.stderr);
            let tail: String = stderr.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            return Err(CoreError::Media(format!("ffmpeg a échoué: {tail}")));
        }
        Ok(())
    }
}
