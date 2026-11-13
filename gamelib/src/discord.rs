//! Discord integration.
//!
//! - **Rich Presence:** while you play, your Discord profile shows "Playing
//!   Hades" with a timer. GameLib talks to the Discord app on this PC over
//!   its local connection (a named pipe on Windows, a socket elsewhere).
//!   Discord needs an *application ID* for this: make a free application
//!   called "GameLib" at <https://discord.com/developers/applications> and
//!   paste its ID into Settings. Nothing goes over the internet from
//!   GameLib; the Discord app does that.
//! - **Webhook:** when a session ends, GameLib can post "You played Hades
//!   for 1h 20m" to a channel. In Discord: Server Settings → Integrations →
//!   Webhooks → New Webhook → Copy Webhook URL, and paste it into Settings.

use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

// --- The local connection ----------------------------------------------------

const OP_HANDSHAKE: u32 = 0;
const OP_FRAME: u32 = 1;
const OP_CLOSE: u32 = 2;

/// A message in Discord's local protocol: an opcode, the length, then JSON.
pub fn encode(opcode: u32, body: &Value) -> Vec<u8> {
    let text = body.to_string();
    let mut out = Vec::with_capacity(8 + text.len());
    out.extend_from_slice(&opcode.to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out
}

/// Read one message: its opcode and JSON.
pub fn read_message(reader: &mut impl Read) -> Result<(u32, Value)> {
    let mut header = [0u8; 8];
    reader
        .read_exact(&mut header)
        .context("Discord closed the connection")?;
    let opcode = u32::from_le_bytes(header[..4].try_into().unwrap_or_default());
    let length = u32::from_le_bytes(header[4..].try_into().unwrap_or_default()) as usize;
    ensure!(
        length < 1 << 20,
        "Discord sent a message that's far too big"
    );
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok((opcode, serde_json::from_slice(&body).unwrap_or(Value::Null)))
}

/// What to show on your profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    /// The game's name (the first line).
    pub details: String,
    /// The second line, e.g. "via GameLib".
    pub state: String,
    /// When play started (Unix time), for Discord's "elapsed" timer.
    pub started_at: i64,
}

/// The SET_ACTIVITY command. `None` clears the activity.
pub fn activity_command(activity: Option<&Activity>, pid: u32, nonce: u64) -> Value {
    let activity = activity.map(|a| {
        json!({
            "details": a.details,
            "state": a.state,
            "timestamps": {"start": a.started_at},
            "instance": false,
        })
    });
    json!({
        "cmd": "SET_ACTIVITY",
        "args": {"pid": pid, "activity": activity},
        "nonce": nonce.to_string(),
    })
}

/// A connection to the Discord app.
enum Pipe {
    #[cfg(windows)]
    File(std::fs::File),
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
}

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(windows)]
            Pipe::File(f) => f.read(buf),
            #[cfg(unix)]
            Pipe::Unix(s) => s.read(buf),
        }
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(windows)]
            Pipe::File(f) => f.write(buf),
            #[cfg(unix)]
            Pipe::Unix(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            #[cfg(windows)]
            Pipe::File(f) => f.flush(),
            #[cfg(unix)]
            Pipe::Unix(s) => s.flush(),
        }
    }
}

/// The places Discord might be listening, in order.
fn candidate_pipes() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    for i in 0..10 {
        out.push(std::path::PathBuf::from(format!(
            r"\\.\pipe\discord-ipc-{i}"
        )));
    }
    #[cfg(unix)]
    {
        let mut dirs: Vec<std::path::PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
            .iter()
            .filter_map(|v| std::env::var_os(v).map(std::path::PathBuf::from))
            .collect();
        dirs.push("/tmp".into());
        for dir in dirs {
            for sub in ["", "app/com.discordapp.Discord", "snap.discord"] {
                for i in 0..10 {
                    out.push(dir.join(sub).join(format!("discord-ipc-{i}")));
                }
            }
        }
    }
    out
}

fn open(path: &std::path::Path) -> Option<Pipe> {
    #[cfg(windows)]
    {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .ok()
            .map(Pipe::File)
    }
    #[cfg(unix)]
    {
        let stream = std::os::unix::net::UnixStream::connect(path).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        Some(Pipe::Unix(stream))
    }
}

/// Connect to the Discord app and introduce GameLib.
fn connect(client_id: &str, pipes: &[std::path::PathBuf]) -> Result<Pipe> {
    for path in pipes {
        let Some(mut pipe) = open(path) else { continue };
        pipe.write_all(&encode(
            OP_HANDSHAKE,
            &json!({"v": 1, "client_id": client_id}),
        ))?;
        let (opcode, reply) = read_message(&mut pipe)?;
        if opcode == OP_CLOSE {
            bail!(
                "Discord refused the application ID: {}",
                reply["message"].as_str().unwrap_or("no reason given")
            );
        }
        ensure!(
            reply["evt"] == "READY",
            "Discord didn't say hello back (is the application ID right?)"
        );
        return Ok(pipe);
    }
    bail!("Discord isn't running on this PC")
}

/// Commands for the Rich Presence thread.
enum Command {
    Set(Activity),
    Clear,
}

/// Rich Presence. The talking happens on its own thread, so a slow or
/// missing Discord never holds up GameLib. Dropping this stops the thread.
pub struct Presence {
    commands: Sender<Command>,
}

impl Presence {
    /// Start, with the Discord application ID from Settings. `on_error`
    /// is told (at most once per problem) when Discord can't be reached.
    pub fn start(client_id: String, on_error: impl Fn(String) + Send + 'static) -> Presence {
        Presence::start_with(client_id, candidate_pipes(), on_error)
    }

    fn start_with(
        client_id: String,
        pipes: Vec<std::path::PathBuf>,
        on_error: impl Fn(String) + Send + 'static,
    ) -> Presence {
        let (commands, inbox) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("gamelib-discord".into())
            .spawn(move || run(client_id, pipes, inbox, on_error));
        if let Err(err) = spawned {
            log::warn!("couldn't start Discord Rich Presence: {err}");
        }
        Presence { commands }
    }

    pub fn playing(&self, activity: Activity) {
        let _ = self.commands.send(Command::Set(activity));
    }

    pub fn clear(&self) {
        let _ = self.commands.send(Command::Clear);
    }
}

fn run(
    client_id: String,
    pipes: Vec<std::path::PathBuf>,
    inbox: Receiver<Command>,
    on_error: impl Fn(String),
) {
    let mut pipe: Option<Pipe> = None;
    let mut nonce = 0u64;
    let mut last_error: Option<String> = None;
    for command in inbox {
        let activity = match &command {
            Command::Set(a) => Some(a.clone()),
            Command::Clear => None,
        };
        // Try twice: the first time may find the old connection gone
        // (Discord restarted).
        let mut outcome = Err(anyhow::anyhow!("not tried"));
        for _ in 0..2 {
            if pipe.is_none() {
                match connect(&client_id, &pipes) {
                    Ok(p) => pipe = Some(p),
                    Err(err) => {
                        outcome = Err(err);
                        break;
                    }
                }
            }
            nonce += 1;
            let message = encode(
                OP_FRAME,
                &activity_command(activity.as_ref(), std::process::id(), nonce),
            );
            let sent = pipe.as_mut().map(|p| {
                p.write_all(&message)?;
                read_message(p).map(|_| ())
            });
            match sent {
                Some(Ok(())) => {
                    outcome = Ok(());
                    break;
                }
                Some(Err(err)) => {
                    pipe = None;
                    outcome = Err(err);
                }
                None => {}
            }
        }
        match outcome {
            Ok(()) => last_error = None,
            Err(err) => {
                let text = format!("{err:#}");
                if last_error.as_ref() != Some(&text) {
                    log::info!("Discord Rich Presence: {text}");
                    on_error(text.clone());
                    last_error = Some(text);
                }
            }
        }
    }
}

// --- Webhook -----------------------------------------------------------------

/// Is this a Discord webhook address?
pub fn is_webhook_url(url: &str) -> bool {
    let url = url.trim();
    [
        "https://discord.com/api/webhooks/",
        "https://discordapp.com/api/webhooks/",
        "https://ptb.discord.com/api/webhooks/",
        "https://canary.discord.com/api/webhooks/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix) && url.len() > prefix.len() + 10)
}

/// The message posted when a session ends.
pub fn session_message(player: &str, game: &str, duration_secs: u64, total_secs: u64) -> Value {
    let length = |secs: u64| {
        let (h, m) = (secs / 3600, secs % 3600 / 60);
        if h > 0 {
            format!("{h}h {m:02}m")
        } else {
            format!("{m}m")
        }
    };
    json!({
        "username": "GameLib",
        "content": format!(
            "🎮 **{player}** played **{game}** for {} (total {}).",
            length(duration_secs),
            length(total_secs)
        ),
        "allowed_mentions": {"parse": []},
    })
}

/// Post a message to a Discord webhook.
pub fn post_webhook(url: &str, body: &Value) -> Result<()> {
    ensure!(is_webhook_url(url), "that isn't a Discord webhook address");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let response = client
        .post(url.trim())
        .json(body)
        .send()
        .context("couldn't reach Discord")?;
    ensure!(
        response.status().is_success(),
        "Discord didn't accept the message ({})",
        response.status()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_framed_and_read_back() {
        let body = json!({"v": 1, "client_id": "123"});
        let bytes = encode(OP_HANDSHAKE, &body);
        assert_eq!(&bytes[..4], &0u32.to_le_bytes());
        let (op, value) = read_message(&mut bytes.as_slice()).unwrap();
        assert_eq!((op, value), (OP_HANDSHAKE, body));
        assert!(read_message(&mut &bytes[..5]).is_err());
    }

    #[test]
    fn activity_commands_look_right() {
        let activity = Activity {
            details: "Hades".into(),
            state: "via GameLib".into(),
            started_at: 1000,
        };
        let set = activity_command(Some(&activity), 42, 7);
        assert_eq!(set["cmd"], "SET_ACTIVITY");
        assert_eq!(set["args"]["pid"], 42);
        assert_eq!(set["args"]["activity"]["details"], "Hades");
        assert_eq!(set["args"]["activity"]["timestamps"]["start"], 1000);
        assert_eq!(set["nonce"], "7");
        assert!(activity_command(None, 42, 8)["args"]["activity"].is_null());
    }

    #[test]
    fn webhooks_are_checked_and_messages_written() {
        assert!(is_webhook_url(
            "https://discord.com/api/webhooks/123456789/abcdefghij"
        ));
        assert!(!is_webhook_url("https://example.com/api/webhooks/123/abc"));
        assert!(!is_webhook_url("https://discord.com/api/webhooks/"));
        let message = session_message("Morgan", "Hades", 4800, 90_000);
        assert_eq!(
            message["content"],
            "🎮 **Morgan** played **Hades** for 1h 20m (total 25h 00m)."
        );
        assert!(
            message["allowed_mentions"]["parse"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    /// A pretend Discord app on a local socket: it answers the handshake,
    /// then records the commands it's sent.
    #[cfg(unix)]
    #[test]
    fn rich_presence_talks_to_discord() {
        use std::os::unix::net::UnixListener;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("discord-ipc-0");
        let listener = UnixListener::bind(&path).unwrap();
        let (seen_tx, seen_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let (op, hello) = read_message(&mut stream).unwrap();
            assert_eq!(op, OP_HANDSHAKE);
            assert_eq!(hello["client_id"], "123");
            stream
                .write_all(&encode(
                    OP_FRAME,
                    &json!({"cmd": "DISPATCH", "evt": "READY"}),
                ))
                .unwrap();
            for _ in 0..2 {
                let (_, command) = read_message(&mut stream).unwrap();
                stream
                    .write_all(&encode(
                        OP_FRAME,
                        &json!({"cmd": "SET_ACTIVITY", "evt": null}),
                    ))
                    .unwrap();
                seen_tx.send(command).unwrap();
            }
        });
        let presence = Presence::start_with("123".into(), vec![path], |err| panic!("{err}"));
        presence.playing(Activity {
            details: "Hades".into(),
            state: "via GameLib".into(),
            started_at: 5,
        });
        presence.clear();
        let first = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(first["args"]["activity"]["details"], "Hades");
        let second = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(second["args"]["activity"].is_null());
    }

    #[test]
    fn a_missing_discord_is_reported_once() {
        let (tx, rx) = mpsc::channel();
        let presence = Presence::start_with(
            "123".into(),
            vec!["/no/such/discord-ipc-0".into()],
            move |err| tx.send(err).unwrap(),
        );
        presence.clear();
        presence.clear();
        let err = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(err.contains("isn't running"), "{err}");
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "only once"
        );
    }
}
