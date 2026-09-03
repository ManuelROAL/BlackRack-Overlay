//! Parser for the session string the simulator publishes alongside telemetry.
//!
//! The string is a restricted YAML dialect: block maps, block sequences of
//! maps, and plain scalars. There are no anchors, flow collections, multi-line
//! scalars or tags, so a line-oriented parser covers it without a dependency,
//! and an unexpected line degrades into a scalar instead of failing the frame.

/// One parsed value. A missing key resolves to `Node::Empty`, so a lookup chain
/// can be written without checking each step.
#[derive(Debug, PartialEq)]
pub(super) enum Node {
    Map(Vec<(String, Node)>),
    List(Vec<Node>),
    Scalar(String),
    Empty,
}

struct Line {
    indent: usize,
    dash: bool,
    text: String,
}

/// Splits the document into logical lines, turning `- key: value` into a dash
/// marker followed by the item's first entry at the indent its siblings use.
fn lines(document: &str) -> Vec<Line> {
    let mut result = Vec::new();
    for raw in document.lines() {
        let trimmed = raw.trim_end();
        let indent = trimmed.len() - trimmed.trim_start().len();
        let text = trimmed.trim_start();
        if text.is_empty() || text == "---" || text == "..." || text.starts_with('#') {
            continue;
        }
        if let Some(item) = text.strip_prefix('-') {
            let content = item.trim_start();
            let offset = text.len() - content.len();
            result.push(Line {
                indent,
                dash: true,
                text: String::new(),
            });
            if !content.is_empty() {
                result.push(Line {
                    indent: indent + offset,
                    dash: false,
                    text: content.to_owned(),
                });
            }
            continue;
        }
        result.push(Line {
            indent,
            dash: false,
            text: text.to_owned(),
        });
    }
    result
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    let mut characters = value.chars();
    match (characters.next(), characters.next_back()) {
        (Some('\''), Some('\'')) | (Some('"'), Some('"')) if value.len() >= 2 => {
            characters.as_str().to_owned()
        }
        _ => value.to_owned(),
    }
}

pub(super) fn parse(document: &str) -> Node {
    let lines = lines(document);
    let mut cursor = 0;
    if lines.is_empty() {
        return Node::Empty;
    }
    parse_block(&lines, &mut cursor, lines[0].indent)
}

fn parse_block(lines: &[Line], cursor: &mut usize, indent: usize) -> Node {
    if lines.get(*cursor).is_some_and(|line| line.dash) {
        parse_list(lines, cursor, indent)
    } else {
        parse_map(lines, cursor, indent)
    }
}

fn parse_list(lines: &[Line], cursor: &mut usize, indent: usize) -> Node {
    let mut items = Vec::new();
    while lines
        .get(*cursor)
        .is_some_and(|line| line.dash && line.indent == indent)
    {
        *cursor += 1;
        let item_indent = lines.get(*cursor).map_or(indent, |line| line.indent);
        if item_indent <= indent {
            // A dash with nothing under it: keep the slot so positions in the
            // sequence still line up with the simulator's numbering.
            items.push(Node::Empty);
            continue;
        }
        items.push(parse_block(lines, cursor, item_indent));
    }
    Node::List(items)
}

fn parse_map(lines: &[Line], cursor: &mut usize, indent: usize) -> Node {
    let mut entries = Vec::new();
    while let Some(line) = lines.get(*cursor) {
        if line.indent != indent || line.dash {
            break;
        }
        let Some((key, value)) = line.text.split_once(':') else {
            // Not a mapping line at all; a lone scalar is the whole block.
            let scalar = Node::Scalar(unquote(&line.text));
            *cursor += 1;
            return if entries.is_empty() {
                scalar
            } else {
                Node::Map(entries)
            };
        };
        let key = key.trim().to_owned();
        let value = value.trim();
        *cursor += 1;
        if !value.is_empty() {
            entries.push((key, Node::Scalar(unquote(value))));
            continue;
        }
        let child = match lines.get(*cursor) {
            Some(next) if next.dash && next.indent == indent => parse_list(lines, cursor, indent),
            Some(next) if next.indent > indent => parse_block(lines, cursor, next.indent),
            _ => Node::Empty,
        };
        entries.push((key, child));
    }
    Node::Map(entries)
}

impl Node {
    pub(super) fn get(&self, key: &str) -> &Node {
        match self {
            Node::Map(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map_or(&Node::Empty, |(_, value)| value),
            _ => &Node::Empty,
        }
    }

    pub(super) fn items(&self) -> &[Node] {
        match self {
            Node::List(items) => items,
            _ => &[],
        }
    }

    pub(super) fn text(&self) -> &str {
        match self {
            Node::Scalar(value) => value,
            _ => "",
        }
    }

    /// Reads a leading number, ignoring any unit the simulator appends, so
    /// `5.891 km` and `3600.0000 sec` both parse.
    pub(super) fn number(&self) -> Option<f64> {
        let text = self.text().trim();
        let end = text
            .find(|character: char| !matches!(character, '0'..='9' | '.' | '-' | '+' | 'e' | 'E'))
            .unwrap_or(text.len());
        text[..end]
            .parse()
            .ok()
            .filter(|value: &f64| value.is_finite())
    }

    pub(super) fn integer(&self) -> Option<i32> {
        self.number().map(|value| value as i32)
    }

    pub(super) fn is_empty(&self) -> bool {
        matches!(self, Node::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, Node};

    const SAMPLE: &str = "---\nWeekendInfo:\n TrackName: spa\n TrackLength: 7.00 km\n WeekendOptions:\n  NumStarters: 24\nSessionInfo:\n Sessions:\n - SessionNum: 0\n   SessionLaps: unlimited\n   SessionTime: 3600.0000 sec\n - SessionNum: 1\n   SessionLaps: 25\nDriverInfo:\n DriverCarIdx: 2\n Drivers:\n - CarIdx: 0\n   UserName: Ana Perez\n   CarClassShortName: GT3\n...\n";

    #[test]
    fn reads_nested_scalars() {
        let document = parse(SAMPLE);
        assert_eq!(document.get("WeekendInfo").get("TrackName").text(), "spa");
        assert_eq!(
            document.get("WeekendInfo").get("TrackLength").number(),
            Some(7.0)
        );
        assert_eq!(
            document
                .get("WeekendInfo")
                .get("WeekendOptions")
                .get("NumStarters")
                .integer(),
            Some(24)
        );
        assert_eq!(
            document.get("DriverInfo").get("DriverCarIdx").integer(),
            Some(2)
        );
    }

    #[test]
    fn reads_sequences_of_maps() {
        let document = parse(SAMPLE);
        let sessions = document.get("SessionInfo").get("Sessions");
        assert_eq!(sessions.items().len(), 2);
        assert_eq!(
            sessions.items()[0].get("SessionTime").number(),
            Some(3600.0)
        );
        assert_eq!(sessions.items()[0].get("SessionLaps").text(), "unlimited");
        assert_eq!(sessions.items()[1].get("SessionLaps").integer(), Some(25));

        let drivers = document.get("DriverInfo").get("Drivers");
        assert_eq!(drivers.items().len(), 1);
        assert_eq!(drivers.items()[0].get("UserName").text(), "Ana Perez");
        assert_eq!(drivers.items()[0].get("CarClassShortName").text(), "GT3");
    }

    #[test]
    fn missing_keys_resolve_to_empty() {
        let document = parse(SAMPLE);
        assert!(document.get("Nope").get("Deeper").is_empty());
        assert_eq!(document.get("Nope").items().len(), 0);
        assert_eq!(document.get("Nope").number(), None);
        assert_eq!(parse(""), Node::Empty);
    }

    #[test]
    fn keeps_values_that_contain_separators() {
        let document = parse("WeekendInfo:\n TrackDisplayName: Circuit: Sector 3\n");
        assert_eq!(
            document.get("WeekendInfo").get("TrackDisplayName").text(),
            "Circuit: Sector 3"
        );
    }

    #[test]
    fn strips_quotes_the_simulator_adds() {
        let document =
            parse("DriverInfo:\n Drivers:\n - UserName: 'Jo Doe'\n   TeamName: \"Team 5\"\n");
        let driver = &document.get("DriverInfo").get("Drivers").items()[0];
        assert_eq!(driver.get("UserName").text(), "Jo Doe");
        assert_eq!(driver.get("TeamName").text(), "Team 5");
    }
}
