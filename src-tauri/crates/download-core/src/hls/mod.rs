//! Téléchargement de flux HLS VOD : parsing de playlist, déchiffrement AES-128,
//! sélection de variante, moteur de download (segments → remux ffmpeg).
pub mod playlist;
pub mod crypto;
pub mod select;
pub mod remux;
pub mod engine;
pub mod live;
