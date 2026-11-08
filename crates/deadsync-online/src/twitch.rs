//! Read-only Twitch IRC over TLS. The caller owns the background worker lifetime.
//!
//! Behavior adapted from Martin Natano's 2022 Simply Love Twitch module.
//! See twitch-LICENSE for the original copyright and permission notice.
use deadsync_config::streaming::TwitchChannel;
use std::collections::{HashMap, VecDeque};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig, stream::MaybeTlsStream};

pub const MAX_MESSAGES: usize = 30;
const MAX_IRC_BYTES: usize = 64 * 1024;
const ENDPOINT: &str = "wss://irc-ws.chat.twitch.tv/";
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessage {
    pub login: String,
    pub display_name: String,
    pub text: String,
    pub color: Option<String>,
    pub badges: Vec<String>,
    pub user_id: Option<String>,
    pub id: Option<String>,
    pub announcement: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Chat(ChatMessage),
    Status {
        text: String,
        notice: Option<String>,
    },
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub entries: Vec<Arc<Entry>>,
    pub connection: Connection,
}

struct History {
    entries: VecDeque<Arc<Entry>>,
    revision: u64,
}

impl History {
    fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_MESSAGES),
            revision: 0,
        }
    }

    fn push(&mut self, entry: Entry) {
        if let Entry::Chat(chat) = &entry
            && let Some(id) = &chat.id
            && self.entries.iter().any(
                |entry| matches!(entry.as_ref(), Entry::Chat(old) if old.id.as_ref() == Some(id)),
            )
        {
            return;
        }
        // Keep a connection outage from filling the entire history with one error.
        if matches!(entry, Entry::Error(_))
            && self
                .entries
                .back()
                .is_some_and(|old| old.as_ref() == &entry)
        {
            return;
        }
        if self.entries.len() == MAX_MESSAGES {
            self.entries.pop_front();
        }
        self.entries.push_back(Arc::new(entry));
        self.revision = self.revision.wrapping_add(1);
    }

    fn status(&mut self, text: &str) {
        self.push(Entry::Status {
            text: text.into(),
            notice: None,
        });
    }

    fn remove(&mut self, predicate: impl Fn(&ChatMessage) -> bool) {
        for entry in &mut self.entries {
            if matches!(entry.as_ref(), Entry::Chat(chat) if predicate(chat)) {
                *entry = Arc::new(Entry::Status {
                    text: "message deleted by a moderator.".into(),
                    notice: None,
                });
                self.revision = self.revision.wrapping_add(1);
            }
        }
    }

    fn snapshot(&self, connection: Connection) -> Snapshot {
        Snapshot {
            entries: self.entries.iter().cloned().collect(),
            connection,
        }
    }
}

struct IrcLine<'a> {
    tags: HashMap<&'a str, String>,
    prefix: &'a str,
    command: &'a str,
    params: Vec<&'a str>,
    trailing: Option<&'a str>,
}

fn unescape_tag(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        if let Some(ch) = chars.next() {
            out.push(match ch {
                's' => ' ',
                ':' => ';',
                'r' => '\r',
                'n' => '\n',
                ch => ch,
            });
        }
    }
    out
}

fn parse_line(mut line: &str) -> Option<IrcLine<'_>> {
    let mut tags = HashMap::new();
    if let Some(tagged) = line.strip_prefix('@') {
        let (raw, rest) = tagged.split_once(' ')?;
        for tag in raw.split(';') {
            let (key, value) = tag.split_once('=').unwrap_or((tag, ""));
            if !value.is_empty() {
                tags.insert(key, unescape_tag(value));
            }
        }
        line = rest;
    }
    let mut prefix = "";
    if let Some(prefixed) = line.strip_prefix(':') {
        let (sender, rest) = prefixed.split_once(' ')?;
        prefix = sender;
        line = rest;
    }
    let (head, trailing) = line
        .split_once(" :")
        .map_or((line, None), |(head, tail)| (head, Some(tail)));
    let mut tokens = head.split_ascii_whitespace();
    let command = tokens.next()?;
    Some(IrcLine {
        tags,
        prefix,
        command,
        params: tokens.collect(),
        trailing,
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    None,
    Pong(String),
    Joined,
    Reconnect,
    Failed(String),
}

fn consume_line(history: &mut History, channel: &str, line: &str) -> Action {
    let Some(line) = parse_line(line) else {
        return Action::None;
    };
    match line.command {
        "PING" => {
            return Action::Pong(format!(
                "PONG :{}\r\n",
                line.trailing
                    .or_else(|| line.params.first().copied())
                    .unwrap_or("tmi.twitch.tv")
            ));
        }
        "RECONNECT" => return Action::Reconnect,
        "NOTICE" if line.params.first() == Some(&"*") => {
            return Action::Failed(line.trailing.unwrap_or("Twitch login failed").into());
        }
        _ => {}
    }
    let target = if line.command == "366" {
        line.params.get(1)
    } else {
        line.params.first()
    };
    if target.and_then(|target| target.strip_prefix('#')) != Some(channel) {
        return Action::None;
    }
    match line.command {
        "ROOMSTATE" | "366" => return Action::Joined,
        "PRIVMSG" | "USERNOTICE" => {
            if line.command == "USERNOTICE"
                && let Some(text) = line.tags.get("system-msg")
            {
                history.push(Entry::Status {
                    text: text.clone(),
                    notice: line.tags.get("msg-id").cloned(),
                });
            }
            if let Some(text) = line.trailing.filter(|text| !text.is_empty()) {
                let login = line
                    .tags
                    .get("login")
                    .map(String::as_str)
                    .unwrap_or_else(|| line.prefix.split('!').next().unwrap_or(""));
                history.push(Entry::Chat(ChatMessage {
                    login: login.into(),
                    display_name: line
                        .tags
                        .get("display-name")
                        .filter(|name| !name.is_empty())
                        .cloned()
                        .unwrap_or_else(|| login.into()),
                    text: text.into(),
                    color: line.tags.get("color").cloned(),
                    badges: line
                        .tags
                        .get("badges")
                        .map(|badges| {
                            badges
                                .split(',')
                                .filter_map(|badge| {
                                    badge.split_once('/').map(|(name, _)| name.into())
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    user_id: line.tags.get("user-id").cloned(),
                    id: line.tags.get("id").cloned(),
                    announcement: line
                        .tags
                        .get("msg-id")
                        .is_some_and(|id| id == "announcement"),
                }));
            }
        }
        "CLEARCHAT" => {
            if let Some(id) = line.tags.get("target-user-id") {
                history.remove(|chat| chat.user_id.as_ref() == Some(id));
            } else {
                history.entries.clear();
                history.status("Chat was cleared by a moderator");
            }
        }
        "CLEARMSG" => {
            if let Some(id) = line.tags.get("target-msg-id") {
                history.remove(|chat| chat.id.as_ref() == Some(id));
            }
        }
        "NOTICE" => {
            return Action::Failed(
                line.trailing
                    .unwrap_or("Unable to join Twitch channel")
                    .into(),
            );
        }
        _ => {}
    }
    Action::None
}

fn connect(cancel: &AtomicBool) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, String> {
    let addresses = ("irc-ws.chat.twitch.tv", 443)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = None;
    let mut error = "Unable to resolve Twitch".to_string();
    for address in addresses {
        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            break;
        }
        match TcpStream::connect_timeout(&address, Duration::from_secs(3)) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(e) => error = e.to_string(),
        }
    }
    let stream = stream.ok_or(error)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_IRC_BYTES))
        .max_frame_size(Some(MAX_IRC_BYTES));
    let (mut socket, _) = tungstenite::client_tls_with_config(ENDPOINT, stream, Some(config), None)
        .map_err(|e| e.to_string())?;
    let stream = match socket.get_mut() {
        MaybeTlsStream::Rustls(stream) => &mut stream.sock,
        MaybeTlsStream::Plain(stream) => stream,
        _ => return Err("Unsupported Twitch TLS transport".into()),
    };
    stream
        .set_read_timeout(Some(POLL_INTERVAL))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| e.to_string())?;
    Ok(socket)
}

fn session(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    channel: &str,
    cancel: &AtomicBool,
    history: &mut History,
    publish: &mut impl FnMut(Snapshot),
) -> Result<(), String> {
    let nick = format!("justinfan{}", rand::random::<u32>());
    // Twitch treats each outgoing WebSocket message as one IRC command.
    // Incoming messages can still contain several CRLF-delimited commands.
    for command in [
        "CAP REQ :twitch.tv/commands twitch.tv/tags\r\n".to_owned(),
        "PASS SCHMOOPIIE\r\n".to_owned(),
        format!("NICK {nick}\r\n"),
        format!("JOIN #{channel}\r\n"),
    ] {
        socket
            .send(Message::Text(command.into()))
            .map_err(|e| e.to_string())?;
    }
    let mut pending = String::new();
    let mut joined = false;
    let start = Instant::now();
    let mut last_received = start;
    while !cancel.load(Ordering::Relaxed) {
        if !joined && start.elapsed() > Duration::from_secs(15) {
            return Err("Channel join timed out; check the channel name".into());
        }
        if last_received.elapsed() > Duration::from_secs(300) {
            return Err("Twitch connection timed out".into());
        }
        let message = match socket.read() {
            Ok(message) => message,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        last_received = Instant::now();
        match message {
            Message::Text(text) => {
                if pending.len() + text.len() > MAX_IRC_BYTES {
                    return Err("Twitch message exceeded the size limit".into());
                }
                pending.push_str(&text);
                let revision = history.revision;
                let mut consumed = 0;
                while let Some(end) = pending[consumed..].find("\r\n") {
                    let end = consumed + end;
                    let action = consume_line(history, channel, &pending[consumed..end]);
                    consumed = end + 2;
                    match action {
                        Action::Pong(reply) => socket
                            .send(Message::Text(reply.into()))
                            .map_err(|e| e.to_string())?,
                        Action::Joined if !joined => {
                            joined = true;
                            history.status("Welcome to the chat room!");
                        }
                        Action::Reconnect => return Err("Twitch requested a reconnect".into()),
                        Action::Failed(reason) => return Err(reason),
                        _ => {}
                    }
                }
                pending.drain(..consumed);
                if history.revision != revision {
                    publish(history.snapshot(if joined {
                        Connection::Connected
                    } else {
                        Connection::Connecting
                    }));
                }
            }
            Message::Ping(_) => socket.flush().map_err(|e| e.to_string())?,
            Message::Close(_) => return Err("Disconnected".into()),
            _ => {}
        }
    }
    let _ = socket.close(None);
    Ok(())
}

/// Blocks only its dedicated worker. Snapshots include all moderation changes;
/// replacing an older snapshot never loses a deletion or clear event.
pub fn run(channel: TwitchChannel, cancel: &AtomicBool, mut publish: impl FnMut(Snapshot)) {
    if channel.is_empty() {
        return;
    }
    let mut history = History::new();
    history.status(&format!("Connecting to #{}…", channel.as_str()));
    publish(history.snapshot(Connection::Connecting));
    let mut backoff = 1_u64;
    while !cancel.load(Ordering::Relaxed) {
        let started = Instant::now();
        let result = connect(cancel).and_then(|mut socket| {
            session(
                &mut socket,
                channel.as_str(),
                cancel,
                &mut history,
                &mut publish,
            )
        });
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if let Err(error) = result {
            history.push(Entry::Error(format!("{error}. Reconnecting…")));
            publish(history.snapshot(Connection::Reconnecting));
        }
        if started.elapsed() > Duration::from_secs(30) {
            backoff = 1;
        }
        let until = Instant::now()
            + Duration::from_millis(backoff * 1000 + u64::from(rand::random::<u16>() % 500));
        while !cancel.load(Ordering::Relaxed) && Instant::now() < until {
            std::thread::sleep(POLL_INTERVAL);
        }
        backoff = (backoff * 2).min(30);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chat(id: &str, user: &str) -> String {
        format!(
            "@id={id};user-id={user};display-name=Somebody;badges=moderator/1;color=#123456 :somebody!u@host PRIVMSG #room :hello 🦀"
        )
    }

    #[test]
    fn twitch_messages_keep_metadata_and_deduplicate() {
        let mut history = History::new();
        consume_line(&mut history, "room", &chat("a", "1"));
        consume_line(&mut history, "room", &chat("a", "1"));
        consume_line(&mut history, "other", &chat("b", "1"));
        assert_eq!(history.entries.len(), 1);
        let Entry::Chat(msg) = history.entries[0].as_ref() else {
            panic!()
        };
        assert_eq!(msg.text, "hello 🦀");
        assert_eq!(msg.display_name, "Somebody");
        assert_eq!(msg.badges, ["moderator"]);
        assert_eq!(msg.color.as_deref(), Some("#123456"));
    }

    #[test]
    fn twitch_moderation_preserves_other_messages_and_bounds_history() {
        let mut history = History::new();
        for i in 0..40 {
            consume_line(
                &mut history,
                "room",
                &chat(&i.to_string(), if i % 2 == 0 { "1" } else { "2" }),
            );
        }
        assert_eq!(history.entries.len(), MAX_MESSAGES);
        consume_line(
            &mut history,
            "room",
            "@target-msg-id=39 :tmi.twitch.tv CLEARMSG #room :hello",
        );
        assert!(matches!(
            history.entries.back().unwrap().as_ref(),
            Entry::Status { .. }
        ));
        consume_line(
            &mut history,
            "room",
            "@target-user-id=1 :tmi.twitch.tv CLEARCHAT #room :somebody",
        );
        assert_eq!(
            history
                .entries
                .iter()
                .filter(|entry| matches!(entry.as_ref(), Entry::Chat(_)))
                .count(),
            14
        );
        consume_line(&mut history, "room", ":tmi.twitch.tv CLEARCHAT #room");
        assert_eq!(history.entries.len(), 1);
    }

    #[test]
    fn twitch_untagged_control_and_full_tag_escaping() {
        assert_eq!(
            unescape_tag(r"hello\sworld\:a\\b\r\n\q\"),
            "hello world;a\\b\r\nq"
        );
        let mut history = History::new();
        assert_eq!(
            consume_line(&mut history, "room", "PING :server"),
            Action::Pong("PONG :server\r\n".into())
        );
        assert_eq!(
            consume_line(&mut history, "room", ":tmi.twitch.tv RECONNECT"),
            Action::Reconnect
        );
        assert_eq!(
            consume_line(&mut history, "room", ":tmi.twitch.tv 366 guest #room :end"),
            Action::Joined
        );
        assert!(matches!(
            consume_line(
                &mut history,
                "room",
                ":tmi.twitch.tv NOTICE * :Login failed"
            ),
            Action::Failed(_)
        ));
        consume_line(
            &mut history,
            "room",
            r"@system-msg=Someone\ssubscribed!;msg-id=sub;display-name=Someone;login=someone :tmi.twitch.tv USERNOTICE #room :Hi!",
        );
        assert_eq!(history.entries.len(), 2);
        assert!(
            matches!(history.entries[0].as_ref(), Entry::Status { text, notice } if text == "Someone subscribed!" && notice.as_deref() == Some("sub"))
        );
    }

    #[test]
    fn twitch_transport_handles_split_lines_ping_and_ordered_moderation() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert_eq!(
                socket.read().unwrap().into_text().unwrap(),
                "CAP REQ :twitch.tv/commands twitch.tv/tags\r\n"
            );
            assert_eq!(
                socket.read().unwrap().into_text().unwrap(),
                "PASS SCHMOOPIIE\r\n"
            );
            let nick = socket.read().unwrap().into_text().unwrap();
            assert!(nick.starts_with("NICK justinfan") && nick.ends_with("\r\n"));
            assert_eq!(
                socket.read().unwrap().into_text().unwrap(),
                "JOIN #room\r\n"
            );
            socket
                .send(Message::Text(
                    format!(":tmi.twitch.tv ROOMSTATE #room\r\n{}\r", chat("a", "1")).into(),
                ))
                .unwrap();
            socket
                .send(Message::Text("\nPING :test\r\n".into()))
                .unwrap();
            assert_eq!(
                socket.read().unwrap().into_text().unwrap(),
                "PONG :test\r\n"
            );
            socket
                .send(Message::Text(
                    "@target-msg-id=a :tmi.twitch.tv CLEARMSG #room :hello\r\n".into(),
                ))
                .unwrap();
            let _ = socket.read();
        });
        let stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let (mut socket, _) =
            tungstenite::client(format!("ws://{address}"), MaybeTlsStream::Plain(stream)).unwrap();
        let cancel = AtomicBool::new(false);
        let mut snapshots = Vec::new();
        session(&mut socket,"room",&cancel,&mut History::new(),&mut |snapshot| {
            let deleted=snapshot.entries.iter().any(|e|matches!(e.as_ref(),Entry::Status {text,..} if text=="message deleted by a moderator."));
            snapshots.push(snapshot);
            if deleted {cancel.store(true,Ordering::Relaxed);}
        }).unwrap();
        server.join().unwrap();
        assert!(snapshots.iter().any(|s| {
            s.entries
                .iter()
                .any(|e| matches!(e.as_ref(), Entry::Chat(_)))
        }));
        assert!(
            !snapshots
                .last()
                .unwrap()
                .entries
                .iter()
                .any(|e| matches!(e.as_ref(), Entry::Chat(_)))
        );
        assert_eq!(snapshots.last().unwrap().connection, Connection::Connected);
    }

    #[test]
    #[ignore = "requires internet; confirms anonymous TLS access without sending chat"]
    fn twitch_live_anonymous_join() {
        let cancel = AtomicBool::new(false);
        let mut socket = connect(&cancel).expect("Twitch TLS handshake");
        let mut connected = false;
        session(
            &mut socket,
            "twitchdev",
            &cancel,
            &mut History::new(),
            &mut |snapshot| {
                if snapshot.connection == Connection::Connected {
                    connected = true;
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .expect("anonymous Twitch join");
        assert!(connected);
    }
}
