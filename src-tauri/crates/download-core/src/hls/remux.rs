use crate::CoreError;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Fichiers d'entrée du remux : déjà concaténés par le moteur (un par piste).
#[derive(Debug, Clone, PartialEq)]
pub struct RemuxInputs {
    pub video: PathBuf,
    pub audio: Option<PathBuf>,
    pub decryption_key: Option<String>,
}

/// Assemble les pistes en un conteneur de sortie (sans ré-encodage).
#[async_trait]
pub trait Remuxer: Send + Sync {
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError>;
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    /// Remuxer de test : écrit un fichier de sortie factice et mémorise les inputs.
    #[derive(Default)]
    pub struct FakeRemuxer {
        pub calls: Mutex<Vec<RemuxInputs>>,
    }

    #[async_trait]
    impl Remuxer for FakeRemuxer {
        async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
            // Concatène les octets des entrées pour simuler un conteneur.
            let mut buf = std::fs::read(&inputs.video)?;
            if let Some(a) = &inputs.audio {
                buf.extend_from_slice(&std::fs::read(a)?);
            }
            std::fs::write(output, buf)?;
            self.calls.lock().unwrap().push(inputs);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeRemuxer;
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn fake_remuxer_writes_output_and_records_call() {
        let dir = tempdir().unwrap();
        let v = dir.path().join("video.cat");
        std::fs::write(&v, b"VIDEO").unwrap();
        let out = dir.path().join("out.mp4");
        let r = FakeRemuxer::default();
        r.remux(RemuxInputs { video: v.clone(), audio: None, decryption_key: None }, &out).await.unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"VIDEO");
        assert_eq!(r.calls.lock().unwrap().len(), 1);
    }
}
