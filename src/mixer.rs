//! Audio-mixer change events, shared by the tiles that mirror the system mixer.
//!
//! PipeWire has no D-Bus interface, so `pactl subscribe` is the event source —
//! the same approach the panel status cluster uses. With it, pressing a volume
//! key (or changing the level from COSMIC's sound settings, or any other app)
//! reaches the popup within milliseconds instead of waiting for the next poll.
//!
//! The hub starts this only while a mixer-following tile is placed and the
//! popup is open. If `pactl` is absent the stream stays silent and the periodic
//! poll still covers us.

use crate::app::Message;
use cosmic::iced::Subscription;

/// The channel a source pushes updates into.
type Out = cosmic::iced::futures::channel::mpsc::Sender<Message>;

/// Emit `Message::MixerChanged` whenever the sink/source state changes. Every
/// `wpctl` call also produces noisy `client` / `sink-input` events, so only the
/// device and server lines are forwarded.
pub fn subscription() -> Subscription<Message> {
    Subscription::run_with("builtin.mixer-events", |_| {
        cosmic::iced::stream::channel(8, |mut output: Out| async move {
            let _ = tokio::task::spawn_blocking(move || {
                let mut child = match std::process::Command::new("pactl")
                    .arg("subscribe")
                    .stdout(std::process::Stdio::piped())
                    .spawn()
                {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let Some(stdout) = child.stdout.take() else { return };
                use std::io::BufRead;
                for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                    if line.contains("sink") || line.contains("source") || line.contains("server") {
                        if output.try_send(Message::MixerChanged).is_err() {
                            break;
                        }
                    }
                }
            })
            .await;
            // Stay pending after the child exits, so iced doesn't restart-loop
            // a source that can't run (e.g. no `pactl`).
            std::future::pending::<()>().await
        })
    })
}
