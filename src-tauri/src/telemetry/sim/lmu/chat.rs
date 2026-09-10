use crate::telemetry::ChatMessage;
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

const CHAT_BRIDGE_MAGIC: u32 = 0x4243_5242;
const CHAT_BRIDGE_VERSION: u32 = 1;
const CHAT_BRIDGE_CAPACITY: usize = 64;
const CHAT_BRIDGE_TEXT_BYTES: usize = 128;
const CHAT_BRIDGE_MAPPING: &str = "Local\\BlackRackOverlay_LMUChat_v1";

const CHAT_MARKER: &str = "[NETLOG] NetComm::PushToChats : ";
const CHAT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const CHAT_PATH_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const CHAT_HISTORY_BYTES: u64 = 128 * 1024;
const MAX_CHAT_MESSAGES: usize = 24;

/// Reads LMU chat through the optional native bridge and falls back to the
/// local trace without adding a network or credentials dependency. The file is
/// tailed incrementally and only while the Chat overlay (or its browser source)
/// is active.
pub(super) struct ChatLog {
    bridge: ChatBridgeReader,
    using_bridge: bool,
    path: Option<PathBuf>,
    position: u64,
    messages: VecDeque<ChatMessage>,
    last_poll: Option<Instant>,
    last_path_refresh: Option<Instant>,
}

impl Default for ChatLog {
    fn default() -> Self {
        Self {
            bridge: ChatBridgeReader::default(),
            using_bridge: false,
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
        if let Some(messages) = self.bridge.update() {
            // A live mapping only proves that the optional plugin started. It
            // may still be unable to receive chat callbacks (for example when
            // another plugin owns that path), so keep the trace fallback until
            // the bridge has actually delivered a message.
            if !messages.is_empty() || self.using_bridge {
                if !self.using_bridge {
                    self.using_bridge = true;
                    self.path = None;
                    self.position = 0;
                    self.messages.clear();
                }
                for message in messages {
                    self.messages.push_back(message);
                    while self.messages.len() > MAX_CHAT_MESSAGES {
                        self.messages.pop_front();
                    }
                }
                return self.messages.iter().cloned().collect();
            }
        }
        self.using_bridge = false;

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

#[repr(C)]
#[derive(Clone, Copy)]
struct ChatBridgeMessage {
    sequence: i32,
    destination: u8,
    _reserved: [u8; 3],
    text: [u8; CHAT_BRIDGE_TEXT_BYTES],
}

#[repr(C)]
struct ChatBridgeMemory {
    magic: u32,
    version: u32,
    active: i32,
    owner_pid: u32,
    write_sequence: i32,
    read_sequence: i32,
    capacity: u32,
    text_bytes: u32,
    messages: [ChatBridgeMessage; CHAT_BRIDGE_CAPACITY],
}

#[cfg(windows)]
struct ChatBridgeMapping {
    handle: windows_sys::Win32::Foundation::HANDLE,
    memory: *mut ChatBridgeMemory,
}

#[cfg(windows)]
impl Drop for ChatBridgeMapping {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Memory::UnmapViewOfFile(
                windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.memory.cast(),
                },
            );
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
struct ChatBridgeReader {
    mapping: Option<ChatBridgeMapping>,
    last_open_attempt: Option<Instant>,
}

#[cfg(windows)]
impl Default for ChatBridgeReader {
    fn default() -> Self {
        Self {
            mapping: None,
            last_open_attempt: None,
        }
    }
}

#[cfg(windows)]
unsafe impl Send for ChatBridgeReader {}

#[cfg(windows)]
impl ChatBridgeReader {
    fn update(&mut self) -> Option<Vec<ChatMessage>> {
        self.ensure_mapping();
        let mapping = self.mapping.as_ref()?;
        let memory = unsafe { &*mapping.memory };
        let valid = unsafe {
            std::ptr::read_volatile(&memory.magic) == CHAT_BRIDGE_MAGIC
                && std::ptr::read_volatile(&memory.version) == CHAT_BRIDGE_VERSION
                && std::ptr::read_volatile(&memory.capacity) == CHAT_BRIDGE_CAPACITY as u32
                && std::ptr::read_volatile(&memory.text_bytes) == CHAT_BRIDGE_TEXT_BYTES as u32
                && std::ptr::read_volatile(&memory.active) != 0
        };
        if !valid {
            return None;
        }

        let write_sequence = unsafe { std::ptr::read_volatile(&memory.write_sequence) };
        let mut read_sequence = unsafe { std::ptr::read_volatile(&memory.read_sequence) };
        let mut messages = Vec::new();
        let oldest_available = write_sequence.saturating_sub(CHAT_BRIDGE_CAPACITY as i32);
        if read_sequence < oldest_available {
            read_sequence = oldest_available;
        }
        while read_sequence < write_sequence {
            let index = (read_sequence as u32 % CHAT_BRIDGE_CAPACITY as u32) as usize;
            let message = unsafe { std::ptr::read_volatile(&memory.messages[index]) };
            if message.sequence != read_sequence {
                read_sequence = read_sequence.saturating_add(1);
                continue;
            }
            if message.destination == 1 {
                let length = message
                    .text
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(message.text.len());
                if let Ok(text) = std::str::from_utf8(&message.text[..length]) {
                    if let Some(chat) = parse_chat_text(text) {
                        messages.push(chat);
                    }
                } else {
                    let text = String::from_utf8_lossy(&message.text[..length]);
                    if let Some(chat) = parse_chat_text(&text) {
                        messages.push(chat);
                    }
                }
            }
            read_sequence = read_sequence.saturating_add(1);
        }
        unsafe {
            std::ptr::write_volatile(&mut (*mapping.memory).read_sequence, read_sequence);
        }
        Some(messages)
    }

    fn ensure_mapping(&mut self) {
        if self.mapping.is_some()
            || self
                .last_open_attempt
                .is_some_and(|last| last.elapsed() < CHAT_PATH_REFRESH_INTERVAL)
        {
            return;
        }
        self.last_open_attempt = Some(Instant::now());

        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::System::Memory::{
            MapViewOfFile, OpenFileMappingW, FILE_MAP_READ, FILE_MAP_WRITE,
        };
        let name = std::ffi::OsStr::new(CHAT_BRIDGE_MAPPING)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let handle = unsafe { OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, 0, name.as_ptr()) };
        if handle.is_null() {
            return;
        }
        let memory = unsafe { MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0) };
        let memory = memory.Value as *mut ChatBridgeMemory;
        if memory.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(handle);
            }
            return;
        }
        self.mapping = Some(ChatBridgeMapping { handle, memory });
    }
}

#[cfg(not(windows))]
#[derive(Default)]
struct ChatBridgeReader;

#[cfg(not(windows))]
impl ChatBridgeReader {
    fn update(&mut self) -> Option<Vec<ChatMessage>> {
        None
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
    let content = line.split_once(CHAT_MARKER)?.1;
    parse_chat_text(content)
}

fn parse_chat_text(content: &str) -> Option<ChatMessage> {
    let content = content.trim();
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
    use super::{parse_chat_line, ChatBridgeMemory, ChatBridgeMessage};

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

    #[test]
    fn native_chat_bridge_layout_matches_the_plugin_contract() {
        assert_eq!(std::mem::size_of::<ChatBridgeMessage>(), 136);
        assert_eq!(std::mem::size_of::<ChatBridgeMemory>(), 8736);
    }
}
