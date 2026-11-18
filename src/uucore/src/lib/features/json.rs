// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=FORMAT`, where FORMAT is `text` (the default) or `json`,
//! and a small JSON writer: the utilities only print JSON, so no parser.

use std::borrow::Cow;
use std::fmt::{self, Write as _};
use std::io::{self, BufWriter, IsTerminal, StdoutLock, Write, stdout};
use std::ops::Index;

use clap::{Arg, ArgMatches};

pub const OUTPUT_FORMAT: &str = "output-format";

/// The `--output-format` argument; the last one given wins.
pub fn output_format_arg(help: String) -> Arg {
    Arg::new(OUTPUT_FORMAT)
        .long(OUTPUT_FORMAT)
        .value_name("FORMAT")
        .value_parser(["text", "json"])
        .overrides_with(OUTPUT_FORMAT)
        .help(help)
}

/// Whether `--output-format=json` was given.
pub fn is_json(matches: &ArgMatches) -> bool {
    matches
        .get_one::<String>(OUTPUT_FORMAT)
        .is_some_and(|format| format == "json")
}

/// Prints `value`, indented on a terminal and on one line otherwise.
pub fn print_json(value: &Value) -> io::Result<()> {
    let mut stdout = stdout().lock();
    let json = value.render(stdout.is_terminal().then_some(0));
    writeln!(stdout, "{json}")
}

/// Prints a JSON array one element at a time, so that a long listing is not
/// held in memory.
pub struct ArrayPrinter {
    out: BufWriter<StdoutLock<'static>>,
    pretty: bool,
    empty: bool,
}

impl ArrayPrinter {
    pub fn new() -> Self {
        let stdout = stdout().lock();
        Self {
            pretty: stdout.is_terminal(),
            out: BufWriter::new(stdout),
            empty: true,
        }
    }

    pub fn push(&mut self, value: &Value) -> io::Result<()> {
        let separator = if self.empty { "[" } else { "," };
        let indent = if self.pretty { "\n  " } else { "" };
        self.empty = false;
        let json = value.render(self.pretty.then_some(1));
        write!(self.out, "{separator}{indent}{json}")
    }

    /// Writes the buffered elements, e.g. before an error message.
    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }

    pub fn finish(mut self) -> io::Result<()> {
        let end = match (self.empty, self.pretty) {
            (true, _) => "[]",
            (false, true) => "\n]",
            (false, false) => "]",
        };
        writeln!(self.out, "{end}")?;
        self.out.flush()
    }
}

impl Default for ArrayPrinter {
    fn default() -> Self {
        Self::new()
    }
}

/// A JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    String(String),
    Array(Vec<Self>),
    Object(Map),
}

/// A JSON object; the keys keep their insertion order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map(Vec<(Cow<'static, str>, Value)>);

impl Map {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<Cow<'static, str>>, value: impl Into<Value>) {
        self.0.push((key.into(), value.into()));
    }
}

impl Value {
    /// An object with these keys, in this order.
    pub fn object<K: Into<Cow<'static, str>>>(
        entries: impl IntoIterator<Item = (K, Self)>,
    ) -> Self {
        Self::Object(entries.into_iter().collect())
    }

    /// The JSON, indented from level `depth`, or on one line for `None`.
    fn render(&self, depth: Option<usize>) -> String {
        let mut out = String::new();
        // Writing to a String cannot fail.
        let _ = self.write(&mut out, depth);
        out
    }

    fn write(&self, out: &mut String, depth: Option<usize>) -> fmt::Result {
        let (open, close, items): (char, char, Vec<(Option<&str>, &Self)>) = match self {
            Self::Null => return out.write_str("null"),
            Self::Bool(value) => return write!(out, "{value}"),
            Self::Int(number) => return write!(out, "{number}"),
            // `{:?}` keeps the decimal point of whole numbers (`2.0`).
            Self::Float(number) => return write!(out, "{number:?}"),
            Self::String(string) => return write_string(out, string),
            Self::Array(values) => ('[', ']', values.iter().map(|v| (None, v)).collect()),
            Self::Object(map) => (
                '{',
                '}',
                map.0.iter().map(|(k, v)| (Some(&**k), v)).collect(),
            ),
        };
        let inner = depth.map(|depth| depth + 1);
        out.push(open);
        for (i, (key, value)) in items.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            if let Some(inner) = inner {
                write!(out, "\n{:width$}", "", width = 2 * inner)?;
            }
            if let Some(key) = key {
                write_string(out, key)?;
                out.push_str(if depth.is_some() { ": " } else { ":" });
            }
            value.write(out, inner)?;
        }
        if let (Some(depth), false) = (depth, items.is_empty()) {
            write!(out, "\n{:width$}", "", width = 2 * depth)?;
        }
        out.write_char(close)
    }
}

fn write_string(out: &mut String, string: &str) -> fmt::Result {
    out.push('"');
    for c in string.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => write!(out, "\\u{:04x}", u32::from(c))?,
            c => out.push(c),
        }
    }
    out.write_char('"')
}

/// The value of `key` in an object, or null.
impl Index<&str> for Value {
    type Output = Self;

    fn index(&self, key: &str) -> &Self {
        static NULL: Value = Value::Null;
        match self {
            Self::Object(map) => map
                .0
                .iter()
                .find(|(k, _)| k == key)
                .map_or(&NULL, |(_, v)| v),
            _ => &NULL,
        }
    }
}

macro_rules! from_integer {
    ($($integer:ty),*) => {
        $(impl From<$integer> for Value {
            fn from(number: $integer) -> Self {
                Self::Int(number as i128)
            }
        })*
    };
}
from_integer!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl From<u128> for Value {
    fn from(number: u128) -> Self {
        i128::try_from(number).map_or(Self::Float(number as f64), Self::Int)
    }
}

/// JSON has no NaN or infinity, which become null.
impl From<f64> for Value {
    fn from(number: f64) -> Self {
        if number.is_finite() {
            Self::Float(number)
        } else {
            Self::Null
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<String> for Value {
    fn from(string: String) -> Self {
        Self::String(string)
    }
}

impl From<&String> for Value {
    fn from(string: &String) -> Self {
        Self::String(string.clone())
    }
}

impl From<&str> for Value {
    fn from(string: &str) -> Self {
        Self::String(string.to_owned())
    }
}

impl From<Cow<'_, str>> for Value {
    fn from(string: Cow<'_, str>) -> Self {
        Self::String(string.into_owned())
    }
}

impl From<Map> for Value {
    fn from(map: Map) -> Self {
        Self::Object(map)
    }
}

/// `None` is null.
impl<T: Into<Self>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

impl<T: Into<Self>> From<Vec<T>> for Value {
    fn from(values: Vec<T>) -> Self {
        values.into_iter().collect()
    }
}

/// An array.
impl<T: Into<Self>> FromIterator<T> for Value {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Self::Array(values.into_iter().map(Into::into).collect())
    }
}

impl<K: Into<Cow<'static, str>>, V: Into<Value>> FromIterator<(K, V)> for Map {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(entries: I) -> Self {
        Self(
            entries
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn test_render() {
        let value = Value::object([
            ("name", Value::from("a\"b\\c\n\u{1}é")),
            ("load", Value::from(2.0)),
            ("none", Value::from(f64::NAN)),
            ("list", Value::from(vec![-1, 2])),
            ("empty", Value::from(Vec::<Value>::new())),
        ]);
        assert_eq!(
            value.render(None),
            r#"{"name":"a\"b\\c\n\u0001é","load":2.0,"none":null,"list":[-1,2],"empty":[]}"#
        );
        assert_eq!(
            Value::from(vec![Value::object([("a", Value::from(true))])]).render(Some(0)),
            "[\n  {\n    \"a\": true\n  }\n]"
        );
    }
}
