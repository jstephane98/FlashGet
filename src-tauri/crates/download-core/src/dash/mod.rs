//! Téléchargement de flux DASH VOD : parsing MPD, index sidx, construction de
//! segments, ClearKey, sélection de variante, moteur (segments → remux ffmpeg).
pub mod clearkey;
pub mod engine;
pub mod mpd;
pub mod segments;
pub mod sidx;
pub mod select;
