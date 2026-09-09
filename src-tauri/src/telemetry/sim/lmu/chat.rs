use crate::telemetry::ChatMessage;
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

const CHAT_MARKER: &str = "[NETLOG] NetComm::PushToChats : ";
const CHAT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const CHAT_PATH_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const CHAT_HISTORY_BYTES: u64 = 128 * 1024;
const MAX_CHAT_MESSAGES: usize = 24;

/// Reads LMU's local chat trace without adding another network or credentials
/// dependency to telemetry. The file is tailed incrementally and only while
/// the Chat overlay (or its browser source) is active.
pub(super) struct ChatLog {
    path: Option<PathBuf>,
    position: u64,
    messages: VecDeque<ChatMessage>,
    last_poll: Option<Instant>,
    last_path_refresh: Option<Instant>,
}

impl Default for ChatLog {
    fn default() -> Self {
        Self {
            path: None,
            position: 0,
            messages: VecDeque::with_capacity(MAX_CHAT_MESSAGES),
            last_poll: None,
            last_path_refresh: None,
        }
    }
}

impl ChatLog {
    pub(super) fn update(&mut self) -> Vec<ChatMessage> {
        let now = Instant::now();
        if self
            .last_poll
            .is_some_and(|last| now.duration_since(last) < CHAT_POLL_INTERVAL)
        {
            return self.messages.iter().cloned().collect();
        }
        self.last_poll = Some(now);

        if self
            .last_path_refresh
            .is_none_or(|last| now.duration_since(last) >= CHAT_PATH_REFRESH_INTERVAL)
        {
            self.last_path_refresh = Some(now);
            let next_path = latest_trace_path();
            if next_path != self.path {
                self.path = next_path;
                self.position = 0;
                self.messages.clear();
                if let Some(path) = self.path.clone() {
                    self.read_initial_tail(&path);
                }
            }
        }

        if let Some(path) = self.path.clone() {
            self.read_appended(&path);
        }
        self.messages.iter().cloned().collect()
    }

    fn read_initial_tail(&mut self, path: &PathBuf) {
        let Ok(mut file) = File::open(path) else {
            return;
        };
        let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
            return;
        };
        self.position = length;
        let start = length.saturating_sub(CHAT_HISTORY_BYTES);
        if file.seek(SeekFrom::Start(start)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return;
        }
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines().skip(usize::from(start > 0)) {
            self.push_line(line);
        }
    }

    fn read_appended(&mut self, path: &PathBuf) {
        let Ok(mut file) = File::open(path) else {
            return;
        };
        let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
            return;
        };
        if length < self.position {
            self.position = 0;
            self.messages.clear();
        }
        if length == self.position || file.seek(SeekFrom::Start(self.position)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return;
        }
        let complete_length = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        if complete_length == 0 {
            return;
        }
        self.position += complete_length as u64;
        let text = String::from_utf8_lossy(&bytes[..complete_length]);
        for line in text.lines() {
            self.push_line(line);
        }
    }

    fn push_line(&mut self, line: &str) {
        if let Some(message) = parse_chat_line(line) {
            self.messages.push_back(message);
            while self.messages.len() > MAX_CHAT_MESSAGES {
                self.messages.pop_front();
            }
        }
    }
}

fn latest_trace_path() -> Option<PathBuf> {
    super::install::installations()
        .into_iter()
        .flat_map(|installation| {
            fs::read_dir(installation.join("UserData/Log"))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
        })
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("trace")
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
        })
        .max_by_key(|entry| {
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let active = u8::from(
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("trace.txt"),
            );
            (modified, active)
        })
        .map(|entry| entry.path())
}

fn parse_chat_line(line: &str) -> Option<ChatMessage> {
    let content = line.split_once(CHAT_MARKER)?.1.trim();
    if content.is_empty() {
        return None;
    }
    let (sender, text) = content
        .split_once(": ")
        .map_or(("RACE CONTROL", content), |(sender, text)| (sender, text));
    let text = text.trim();
    (!text.is_empty()).then(|| ChatMessage {
        sender: sender.trim().to_owned(),
        text: text.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_chat_line;

    #[test]
    fn parses_driver_chat() {
        let message = parse_chat_line(
            " 898.01s NetCommClien 6900: [NETLOG] NetComm::PushToChats : K Niedzwiecki: Sorry",
        )
        .unwrap();
        assert_eq!(message.sender, "K Niedzwiecki");
        assert_eq!(message.text, "Sorry");
    }

    #[test]
    fn keeps_system_chat_readable() {
        let message = parse_chat_line(
            "100.00s NetCommClien 6900: [NETLOG] NetComm::PushToChats : Driver joined #7 Team",
        )
        .unwrap();
        assert_eq!(message.sender, "RACE CONTROL");
        assert_eq!(message.text, "Driver joined #7 Team");
    }

    #[test]
    fn ignores_unrelated_lines() {
        assert!(parse_chat_line("[NETLOG] NetComm::PushToChat : nope").is_none());
    }
}
