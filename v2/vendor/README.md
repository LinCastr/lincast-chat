# Vendored crates

Local copies of LiveKit's `libwebrtc` 0.3.51 and `webrtc-sys` 0.3.48
(Apache-2.0, see each crate's `Cargo.toml` and `webrtc-sys/NOTICE.md`), used
through `[patch.crates-io]` in the workspace `Cargo.toml`.

Changes from the published crates (2026-10-21):

- `webrtc-sys`: `AudioTrack::set_volume(double)` in `include/livekit/audio_track.h`,
  `src/audio_track.cpp` and the `src/audio_track.rs` bridge. It calls
  `AudioSourceInterface::SetVolume`, the playout gain of a remote track (0 to 10).
- `libwebrtc`: `RtcAudioTrack::set_volume` in `src/audio_track.rs` and
  `src/native/audio_track.rs`, exposing it.

Without it the app could only mute a person (`set_enabled`); per-person volume
up to 400% and the call output volume need the gain.
