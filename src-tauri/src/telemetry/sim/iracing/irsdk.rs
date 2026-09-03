//! Reader for the simulator's shared-memory telemetry interface.
//!
//! The mapping holds a fixed header, a table describing every telemetry
//! variable, a small ring of value buffers and a YAML session string. Nothing
//! here interprets a value: it keeps the newest buffer plus the variable index
//! needed to read a name out of it, and hands the session text to `session.rs`.
//!
//! Layout constants come from the publicly distributed SDK header. They are
//! byte offsets rather than a `#[repr(C)]` mirror so a struct-packing change on
//! this side can never silently reinterpret the mapping. Everything that reads
//! those offsets works on a plain byte slice, which keeps the decoding testable
//! without the simulator.

use std::collections::HashMap;
use std::ffi::c_void;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, VirtualQuery, FILE_MAP_READ,
    MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS,
};

/// The mapping only exists while the simulator runs, which makes opening it the
/// availability probe for this source.
const MAPPING_NAME: &str = "Local\\IRSDKMemMapFileName";

const HEADER_STATUS: usize = 4;
const HEADER_SESSION_UPDATE: usize = 12;
const HEADER_SESSION_LEN: usize = 16;
const HEADER_SESSION_OFFSET: usize = 20;
const HEADER_NUM_VARS: usize = 24;
const HEADER_VAR_HEADER_OFFSET: usize = 28;
const HEADER_NUM_BUF: usize = 32;
const HEADER_BUF_LEN: usize = 36;
const HEADER_VAR_BUF: usize = 48;
const VAR_BUF_STRIDE: usize = 16;
const VAR_HEADER_STRIDE: usize = 144;
const VAR_HEADER_NAME: usize = 16;
const VAR_NAME_LENGTH: usize = 32;
const MAX_BUFFERS: usize = 4;
const MAX_VARIABLES: usize = 4_096;
const MAX_SESSION_LENGTH: usize = 4 * 1024 * 1024;
/// Smallest mapping that can carry a complete header.
const MINIMUM_LENGTH: usize = HEADER_VAR_BUF + MAX_BUFFERS * VAR_BUF_STRIDE;
const STATUS_CONNECTED: i32 = 1;

const TYPE_CHAR: i32 = 0;
const TYPE_BOOL: i32 = 1;
const TYPE_INT: i32 = 2;
const TYPE_BITFIELD: i32 = 3;
const TYPE_FLOAT: i32 = 4;
const TYPE_DOUBLE: i32 = 5;

const fn value_size(kind: i32) -> usize {
    match kind {
        TYPE_CHAR | TYPE_BOOL => 1,
        TYPE_DOUBLE => 8,
        _ => 4,
    }
}

fn bytes(mapping: &[u8], offset: usize, length: usize) -> Option<&[u8]> {
    let end = offset.checked_add(length)?;
    mapping.get(offset..end)
}

fn header(mapping: &[u8], offset: usize) -> Option<i32> {
    let value = bytes(mapping, offset, 4)?;
    Some(i32::from_le_bytes(value.try_into().ok()?))
}

#[derive(Clone, Copy)]
struct Variable {
    kind: i32,
    offset: usize,
    count: usize,
}

/// Everything read out of the mapping: the variable index, the session string
/// and a copy of the newest value buffer.
#[derive(Default)]
struct Snapshot {
    variables: HashMap<String, Variable>,
    variable_layout: (i32, i32),
    session_generation: i32,
    session_text: String,
    values: Vec<u8>,
}

impl Snapshot {
    fn new() -> Self {
        Self {
            session_generation: i32::MIN,
            ..Self::default()
        }
    }

    /// Copies the newest value buffer and refreshes the variable index and the
    /// session string. Returns false when the simulator is not publishing, in
    /// which case the previous values are dropped rather than served stale.
    fn refresh(&mut self, mapping: &[u8]) -> bool {
        if !connected(mapping) {
            self.values.clear();
            return false;
        }
        self.refresh_variables(mapping);
        self.refresh_session(mapping);
        self.refresh_values(mapping)
    }

    fn refresh_variables(&mut self, mapping: &[u8]) {
        let (Some(count), Some(table)) = (
            header(mapping, HEADER_NUM_VARS),
            header(mapping, HEADER_VAR_HEADER_OFFSET),
        ) else {
            return;
        };
        if self.variable_layout == (count, table) && !self.variables.is_empty() {
            return;
        }
        self.variable_layout = (count, table);
        self.variables.clear();
        if count <= 0 || table < 0 {
            return;
        }
        let table = table as usize;
        for index in 0..(count as usize).min(MAX_VARIABLES) {
            let entry = table + index * VAR_HEADER_STRIDE;
            let (Some(kind), Some(offset), Some(items)) = (
                header(mapping, entry),
                header(mapping, entry + 4),
                header(mapping, entry + 8),
            ) else {
                break;
            };
            let Some(raw) = bytes(mapping, entry + VAR_HEADER_NAME, VAR_NAME_LENGTH) else {
                break;
            };
            let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
            let Ok(name) = std::str::from_utf8(&raw[..end]) else {
                continue;
            };
            if name.is_empty()
                || offset < 0
                || items <= 0
                || !(TYPE_CHAR..=TYPE_DOUBLE).contains(&kind)
            {
                continue;
            }
            self.variables.insert(
                name.to_owned(),
                Variable {
                    kind,
                    offset: offset as usize,
                    count: items as usize,
                },
            );
        }
    }

    fn refresh_session(&mut self, mapping: &[u8]) {
        let (Some(generation), Some(offset), Some(length)) = (
            header(mapping, HEADER_SESSION_UPDATE),
            header(mapping, HEADER_SESSION_OFFSET),
            header(mapping, HEADER_SESSION_LEN),
        ) else {
            return;
        };
        if generation == self.session_generation || offset < 0 || length <= 0 {
            return;
        }
        let Some(text) = bytes(
            mapping,
            offset as usize,
            (length as usize).min(MAX_SESSION_LENGTH),
        ) else {
            return;
        };
        let end = text
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(text.len());
        // Driver and team names are free text and are not guaranteed UTF-8, so
        // an invalid byte replaces one character instead of losing the roster.
        self.session_text = String::from_utf8_lossy(&text[..end]).into_owned();
        self.session_generation = generation;
    }

    fn refresh_values(&mut self, mapping: &[u8]) -> bool {
        let (Some(buffers), Some(row)) = (
            header(mapping, HEADER_NUM_BUF),
            header(mapping, HEADER_BUF_LEN),
        ) else {
            return false;
        };
        if row <= 0 {
            return false;
        }
        let row = row as usize;
        let buffers = (buffers.max(0) as usize).min(MAX_BUFFERS);
        let mut newest: Option<(i32, usize, usize)> = None;
        for index in 0..buffers {
            let entry = HEADER_VAR_BUF + index * VAR_BUF_STRIDE;
            let (Some(tick), Some(offset)) = (header(mapping, entry), header(mapping, entry + 4))
            else {
                continue;
            };
            if offset < 0 {
                continue;
            }
            if newest.is_none_or(|(best, _, _)| tick > best) {
                newest = Some((tick, offset as usize, entry));
            }
        }
        let Some((tick, offset, entry)) = newest else {
            return false;
        };
        let Some(values) = bytes(mapping, offset, row) else {
            return false;
        };
        // The simulator writes the buffer while we read it, so the copy is only
        // adopted when the tick it belongs to did not advance meanwhile.
        let copied = values.to_vec();
        if header(mapping, entry) != Some(tick) {
            return !self.values.is_empty();
        }
        self.values = copied;
        true
    }

    /// The bytes of a variable's first entry. Some variables are arrays, one
    /// entry per car slot; reading those by index arrives with the roster.
    fn slot(&self, name: &str) -> Option<(i32, &[u8])> {
        let variable = self.variables.get(name)?;
        if variable.count == 0 {
            return None;
        }
        let size = value_size(variable.kind);
        let end = variable.offset.checked_add(size)?;
        Some((variable.kind, self.values.get(variable.offset..end)?))
    }

    fn number(&self, name: &str) -> Option<f64> {
        let (kind, bytes) = self.slot(name)?;
        let value = match kind {
            TYPE_DOUBLE => f64::from_le_bytes(bytes.try_into().ok()?),
            TYPE_FLOAT => f64::from(f32::from_le_bytes(bytes.try_into().ok()?)),
            TYPE_INT | TYPE_BITFIELD => f64::from(i32::from_le_bytes(bytes.try_into().ok()?)),
            _ => f64::from(bytes[0]),
        };
        value.is_finite().then_some(value)
    }

    fn integer(&self, name: &str) -> Option<i32> {
        let (kind, bytes) = self.slot(name)?;
        Some(match kind {
            TYPE_DOUBLE => f64::from_le_bytes(bytes.try_into().ok()?) as i32,
            TYPE_FLOAT => f32::from_le_bytes(bytes.try_into().ok()?) as i32,
            TYPE_INT | TYPE_BITFIELD => i32::from_le_bytes(bytes.try_into().ok()?),
            _ => i32::from(bytes[0]),
        })
    }
}

/// True while the simulator says the mapping carries live telemetry. It stays
/// mapped but goes unconnected whenever the session ends.
fn connected(mapping: &[u8]) -> bool {
    header(mapping, HEADER_STATUS).is_some_and(|status| status & STATUS_CONNECTED != 0)
}

/// An open view over the simulator's mapping plus the newest values read from
/// it. The view is owned by this struct and only ever read, so it is safe to
/// move to the telemetry loop thread with the rest of the source.
pub(super) struct Connection {
    mapping: HANDLE,
    view: *const u8,
    length: usize,
    snapshot: Snapshot,
}

unsafe impl Send for Connection {}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.view as *mut c_void,
            });
            CloseHandle(self.mapping);
        }
    }
}

/// Whether the simulator is publishing telemetry at all, without holding the
/// mapping open. This is the probe the source selection uses.
pub(super) fn available() -> bool {
    Connection::open().is_some()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

impl Connection {
    pub(super) fn open() -> Option<Self> {
        let name = wide(MAPPING_NAME);
        let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr()) };
        if mapping.is_null() {
            return None;
        }
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0) };
        if view.Value.is_null() {
            unsafe { CloseHandle(mapping) };
            return None;
        }
        // The mapping is created by the simulator, so its size is only known by
        // asking the memory manager. Every later read is bounded by it.
        let mut region = unsafe { std::mem::zeroed::<MEMORY_BASIC_INFORMATION>() };
        let queried = unsafe {
            VirtualQuery(
                view.Value,
                &mut region,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        let length = if queried == 0 { 0 } else { region.RegionSize };
        if length < MINIMUM_LENGTH {
            unsafe {
                UnmapViewOfFile(view);
                CloseHandle(mapping);
            }
            return None;
        }
        Some(Self {
            mapping,
            view: view.Value as *const u8,
            length,
            snapshot: Snapshot::new(),
        })
    }

    /// Reads the mapping into the snapshot, and reports whether the simulator
    /// is publishing. The slice describes memory the simulator owns rather than
    /// anything borrowed from `self`, so it is rebuilt per read instead of
    /// stored, which is also what lets the snapshot be taken mutably here.
    pub(super) fn refresh(&mut self) -> bool {
        let region = unsafe { std::slice::from_raw_parts(self.view, self.length) };
        self.snapshot.refresh(region)
    }

    pub(super) fn session_text(&self) -> &str {
        &self.snapshot.session_text
    }

    pub(super) fn session_generation(&self) -> i32 {
        self.snapshot.session_generation
    }

    /// Reads any numeric variable as `f64` so a caller does not have to know
    /// whether the simulator publishes a value as float, double or integer.
    pub(super) fn number(&self, name: &str) -> Option<f64> {
        self.snapshot.number(name)
    }

    pub(super) fn integer(&self, name: &str) -> Option<i32> {
        self.snapshot.integer(name)
    }

    pub(super) fn flag(&self, name: &str) -> Option<bool> {
        self.integer(name).map(|value| value != 0)
    }

    pub(super) fn bits(&self, name: &str) -> u32 {
        self.integer(name).unwrap_or(0) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Snapshot, HEADER_BUF_LEN, HEADER_NUM_BUF, HEADER_NUM_VARS, HEADER_SESSION_LEN,
        HEADER_SESSION_OFFSET, HEADER_SESSION_UPDATE, HEADER_STATUS, HEADER_VAR_BUF,
        HEADER_VAR_HEADER_OFFSET, MINIMUM_LENGTH, TYPE_BOOL, TYPE_DOUBLE, TYPE_FLOAT, TYPE_INT,
        VAR_BUF_STRIDE, VAR_HEADER_NAME, VAR_HEADER_STRIDE, VAR_NAME_LENGTH,
    };

    struct Value {
        name: &'static str,
        kind: i32,
        bytes: Vec<u8>,
    }

    fn float(name: &'static str, value: f32) -> Value {
        Value {
            name,
            kind: TYPE_FLOAT,
            bytes: value.to_le_bytes().to_vec(),
        }
    }

    fn double(name: &'static str, value: f64) -> Value {
        Value {
            name,
            kind: TYPE_DOUBLE,
            bytes: value.to_le_bytes().to_vec(),
        }
    }

    fn integer(name: &'static str, value: i32) -> Value {
        Value {
            name,
            kind: TYPE_INT,
            bytes: value.to_le_bytes().to_vec(),
        }
    }

    fn boolean(name: &'static str, value: bool) -> Value {
        Value {
            name,
            kind: TYPE_BOOL,
            bytes: vec![u8::from(value)],
        }
    }

    fn write(mapping: &mut [u8], offset: usize, value: i32) {
        mapping[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Builds a mapping laid out the way the simulator does: header, variable
    /// table, session string, then the value buffers.
    fn mapping(values: &[Value], session: &str, buffers: &[(i32, usize)]) -> Vec<u8> {
        let table = MINIMUM_LENGTH;
        let session_offset = table + values.len() * VAR_HEADER_STRIDE;
        let row: usize = values.iter().map(|value| value.bytes.len()).sum();
        let first_buffer = session_offset + session.len() + 1;
        let mut result = vec![0_u8; first_buffer + buffers.len() * row.max(1)];

        write(&mut result, HEADER_STATUS, 1);
        write(&mut result, HEADER_SESSION_UPDATE, 7);
        write(&mut result, HEADER_SESSION_OFFSET, session_offset as i32);
        write(&mut result, HEADER_SESSION_LEN, session.len() as i32 + 1);
        write(&mut result, HEADER_NUM_VARS, values.len() as i32);
        write(&mut result, HEADER_VAR_HEADER_OFFSET, table as i32);
        write(&mut result, HEADER_NUM_BUF, buffers.len() as i32);
        write(&mut result, HEADER_BUF_LEN, row as i32);
        result[session_offset..session_offset + session.len()].copy_from_slice(session.as_bytes());

        let mut value_offset = 0;
        for (index, value) in values.iter().enumerate() {
            let entry = table + index * VAR_HEADER_STRIDE;
            write(&mut result, entry, value.kind);
            write(&mut result, entry + 4, value_offset as i32);
            write(&mut result, entry + 8, 1);
            let name = entry + VAR_HEADER_NAME;
            result[name..name + value.name.len()].copy_from_slice(value.name.as_bytes());
            value_offset += value.bytes.len();
        }

        for (index, (tick, buffer)) in buffers.iter().enumerate() {
            let entry = HEADER_VAR_BUF + index * VAR_BUF_STRIDE;
            let offset = first_buffer + buffer * row;
            write(&mut result, entry, *tick);
            write(&mut result, entry + 4, offset as i32);
            let mut cursor = offset;
            for value in values {
                result[cursor..cursor + value.bytes.len()].copy_from_slice(&value.bytes);
                cursor += value.bytes.len();
            }
        }
        result
    }

    #[test]
    fn reads_each_published_value_type() {
        let values = [
            float("Speed", 55.5),
            double("SessionTime", 1_234.5),
            integer("Gear", 4),
            boolean("OnPitRoad", true),
        ];
        let bytes = mapping(&values, "WeekendInfo:\n", &[(1, 0)]);
        let mut snapshot = Snapshot::new();

        assert!(snapshot.refresh(&bytes));
        assert_eq!(snapshot.number("Speed"), Some(55.5));
        assert_eq!(snapshot.number("SessionTime"), Some(1_234.5));
        assert_eq!(snapshot.integer("Gear"), Some(4));
        assert_eq!(snapshot.integer("OnPitRoad"), Some(1));
        // A float read as an integer truncates, and an integer read as a number
        // widens, so a caller never has to know the published type.
        assert_eq!(snapshot.integer("Speed"), Some(55));
        assert_eq!(snapshot.number("Gear"), Some(4.0));
        assert_eq!(snapshot.number("Missing"), None);
        assert_eq!(snapshot.integer("Missing"), None);
    }

    #[test]
    fn reads_the_session_string_once_per_generation() {
        let values = [integer("Gear", 1)];
        let mut bytes = mapping(&values, "WeekendInfo:\n TrackName: spa\n", &[(1, 0)]);
        let mut snapshot = Snapshot::new();

        snapshot.refresh(&bytes);
        assert_eq!(snapshot.session_generation, 7);
        assert!(snapshot.session_text.contains("TrackName: spa"));

        // Same generation, different text: the published text is not reread.
        let offset = super::header(&bytes, HEADER_SESSION_OFFSET).unwrap() as usize;
        bytes[offset] = b'X';
        snapshot.refresh(&bytes);
        assert!(snapshot.session_text.starts_with("WeekendInfo"));

        write(&mut bytes, HEADER_SESSION_UPDATE, 8);
        snapshot.refresh(&bytes);
        assert!(snapshot.session_text.starts_with("XeekendInfo"));
    }

    #[test]
    fn takes_the_buffer_with_the_highest_tick() {
        let values = [integer("Lap", 3)];
        let mut bytes = mapping(&values, "", &[(4, 0), (9, 1), (2, 2)]);
        // Give each buffer its own value so the chosen one is identifiable.
        let second = super::header(&bytes, HEADER_VAR_BUF + VAR_BUF_STRIDE + 4).unwrap() as usize;
        write(&mut bytes, second, 42);

        let mut snapshot = Snapshot::new();
        assert!(snapshot.refresh(&bytes));
        assert_eq!(snapshot.integer("Lap"), Some(42));
    }

    #[test]
    fn reports_nothing_while_the_simulator_is_disconnected() {
        let values = [integer("Lap", 3)];
        let mut bytes = mapping(&values, "", &[(1, 0)]);
        let mut snapshot = Snapshot::new();
        assert!(snapshot.refresh(&bytes));

        write(&mut bytes, HEADER_STATUS, 0);
        assert!(!snapshot.refresh(&bytes));
        assert_eq!(snapshot.integer("Lap"), None);
    }

    #[test]
    fn refuses_offsets_that_fall_outside_the_mapping() {
        let values = [integer("Lap", 3)];

        // A variable table past the end leaves the index empty instead of
        // reading beyond the region.
        let mut beyond = mapping(&values, "", &[(1, 0)]);
        let past_the_end = beyond.len() as i32;
        write(&mut beyond, HEADER_VAR_HEADER_OFFSET, past_the_end);
        let mut snapshot = Snapshot::new();
        snapshot.refresh(&beyond);
        assert_eq!(snapshot.integer("Lap"), None);

        // A row longer than the mapping yields no values at all.
        let mut oversized = mapping(&values, "", &[(1, 0)]);
        let longer_than_the_mapping = oversized.len() as i32;
        write(&mut oversized, HEADER_BUF_LEN, longer_than_the_mapping);
        let mut snapshot = Snapshot::new();
        assert!(!snapshot.refresh(&oversized));
        assert_eq!(snapshot.integer("Lap"), None);
    }

    #[test]
    fn ignores_a_variable_whose_name_is_not_terminated_text() {
        let values = [integer("Lap", 3)];
        let mut bytes = mapping(&values, "", &[(1, 0)]);
        let table = super::header(&bytes, HEADER_VAR_HEADER_OFFSET).unwrap() as usize;
        let name = table + VAR_HEADER_NAME;
        bytes[name..name + VAR_NAME_LENGTH].fill(0xFF);

        let mut snapshot = Snapshot::new();
        snapshot.refresh(&bytes);
        assert_eq!(snapshot.integer("Lap"), None);
    }
}
