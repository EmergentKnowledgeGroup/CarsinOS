//! Strict canonical JSON used by ExecAss receipts.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalValue {
    Null,
    Bool(bool),
    Integer(i64),
    String(String),
    Array(Vec<CanonicalValue>),
    Object(BTreeMap<String, CanonicalValue>),
}

impl CanonicalValue {
    pub fn string(value: impl AsRef<str>) -> Self {
        Self::String(normalize(value.as_ref()))
    }

    pub fn object(entries: Vec<(String, CanonicalValue)>) -> Result<Self> {
        let mut object = BTreeMap::new();
        for (key, value) in entries {
            let key = normalize(&key);
            if object.insert(key, value).is_some() {
                bail!("canonical object contains a duplicate normalized key");
            }
        }
        Ok(Self::Object(object))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut output = Vec::new();
        write_value(self, &mut output);
        output
    }
}

pub fn parse_strict_json(input: &str) -> Result<CanonicalValue> {
    if input.as_bytes().starts_with(&[0xef, 0xbb, 0xbf]) {
        bail!("canonical JSON must not contain a BOM");
    }
    let mut parser = Parser {
        bytes: input.as_bytes(),
        offset: 0,
        depth: 0,
    };
    let value = parser.value()?;
    parser.whitespace();
    if parser.offset != parser.bytes.len() {
        bail!("canonical JSON has trailing data");
    }
    Ok(value)
}

fn normalize(value: &str) -> String {
    value.nfc().collect()
}

fn write_value(value: &CanonicalValue, output: &mut Vec<u8>) {
    match value {
        CanonicalValue::Null => output.extend_from_slice(b"null"),
        CanonicalValue::Bool(value) => {
            output.extend_from_slice(if *value { b"true" } else { b"false" })
        }
        CanonicalValue::Integer(value) => output.extend_from_slice(value.to_string().as_bytes()),
        CanonicalValue::String(value) => write_string(value, output),
        CanonicalValue::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_value(value, output);
            }
            output.push(b']');
        }
        CanonicalValue::Object(values) => {
            output.push(b'{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_string(key, output);
                output.push(b':');
                write_value(value, output);
            }
            output.push(b'}');
        }
    }
}

fn write_string(value: &str, output: &mut Vec<u8>) {
    output.push(b'"');
    for character in value.chars() {
        match character {
            '"' => output.extend_from_slice(br#"\""#),
            '\\' => output.extend_from_slice(br#"\\"#),
            '\u{08}' => output.extend_from_slice(br#"\b"#),
            '\u{0c}' => output.extend_from_slice(br#"\f"#),
            '\n' => output.extend_from_slice(br#"\n"#),
            '\r' => output.extend_from_slice(br#"\r"#),
            '\t' => output.extend_from_slice(br#"\t"#),
            character if character <= '\u{1f}' => {
                output.extend_from_slice(format!("\\u{:04x}", character as u32).as_bytes());
            }
            character => {
                let mut buffer = [0_u8; 4];
                output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    output.push(b'"');
}

struct Parser<'a> {
    bytes: &'a [u8],
    offset: usize,
    depth: usize,
}

impl Parser<'_> {
    fn value(&mut self) -> Result<CanonicalValue> {
        self.whitespace();
        if self.depth >= 32 {
            bail!("canonical JSON nesting exceeds 32 levels");
        }
        self.depth += 1;
        let result = match self.peek() {
            Some(b'n') => {
                self.literal(b"null")?;
                Ok(CanonicalValue::Null)
            }
            Some(b't') => {
                self.literal(b"true")?;
                Ok(CanonicalValue::Bool(true))
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(CanonicalValue::Bool(false))
            }
            Some(b'"') => self.string().map(CanonicalValue::String),
            Some(b'[') => self.array(),
            Some(b'{') => self.object(),
            Some(b'-' | b'0'..=b'9') => self.integer().map(CanonicalValue::Integer),
            _ => bail!("canonical JSON contains an invalid value"),
        };
        self.depth -= 1;
        result
    }

    fn array(&mut self) -> Result<CanonicalValue> {
        self.take(b'[')?;
        let mut values = Vec::new();
        self.whitespace();
        if self.consume(b']') {
            return Ok(CanonicalValue::Array(values));
        }
        loop {
            if values.len() >= 1024 {
                bail!("canonical JSON array exceeds 1024 elements");
            }
            values.push(self.value()?);
            self.whitespace();
            if self.consume(b']') {
                break;
            }
            self.take(b',')?;
        }
        Ok(CanonicalValue::Array(values))
    }

    fn object(&mut self) -> Result<CanonicalValue> {
        self.take(b'{')?;
        let mut values = BTreeMap::new();
        self.whitespace();
        if self.consume(b'}') {
            return Ok(CanonicalValue::Object(values));
        }
        loop {
            if values.len() >= 256 {
                bail!("canonical JSON object exceeds 256 keys");
            }
            self.whitespace();
            let key = self.string()?;
            self.whitespace();
            self.take(b':')?;
            let value = self.value()?;
            if values.insert(key, value).is_some() {
                bail!("canonical JSON contains a duplicate normalized key");
            }
            self.whitespace();
            if self.consume(b'}') {
                break;
            }
            self.take(b',')?;
        }
        Ok(CanonicalValue::Object(values))
    }

    fn integer(&mut self) -> Result<i64> {
        let start = self.offset;
        let negative = self.consume(b'-');
        if self.consume(b'0') {
            if negative {
                bail!("canonical JSON forbids negative zero");
            }
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                bail!("canonical JSON forbids leading zeroes");
            }
        } else {
            let first = self
                .peek()
                .context("canonical JSON integer is incomplete")?;
            if !(b'1'..=b'9').contains(&first) {
                bail!("canonical JSON integer is invalid");
            }
            self.offset += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.offset += 1;
            }
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E' | b'+')) {
            bail!("canonical JSON forbids floats and exponents");
        }
        std::str::from_utf8(&self.bytes[start..self.offset])?
            .parse::<i64>()
            .context("canonical JSON integer exceeds i64")
    }

    fn string(&mut self) -> Result<String> {
        self.take(b'"')?;
        let mut output = String::new();
        loop {
            let byte = self
                .peek()
                .context("canonical JSON string is unterminated")?;
            if byte == b'"' {
                self.offset += 1;
                break;
            }
            if byte == b'\\' {
                self.offset += 1;
                match self.peek().context("canonical JSON escape is incomplete")? {
                    b'"' => {
                        output.push('"');
                        self.offset += 1;
                    }
                    b'\\' => {
                        output.push('\\');
                        self.offset += 1;
                    }
                    b'/' => {
                        output.push('/');
                        self.offset += 1;
                    }
                    b'b' => {
                        output.push('\u{08}');
                        self.offset += 1;
                    }
                    b'f' => {
                        output.push('\u{0c}');
                        self.offset += 1;
                    }
                    b'n' => {
                        output.push('\n');
                        self.offset += 1;
                    }
                    b'r' => {
                        output.push('\r');
                        self.offset += 1;
                    }
                    b't' => {
                        output.push('\t');
                        self.offset += 1;
                    }
                    b'u' => {
                        self.offset += 1;
                        self.unicode_escape(&mut output)?;
                    }
                    _ => bail!("canonical JSON escape is invalid"),
                }
            } else {
                if byte < 0x20 {
                    bail!("canonical JSON string contains an unescaped control byte");
                }
                let tail = std::str::from_utf8(&self.bytes[self.offset..])?;
                let character = tail
                    .chars()
                    .next()
                    .context("canonical JSON string is invalid")?;
                output.push(character);
                self.offset += character.len_utf8();
            }
        }
        Ok(normalize(&output))
    }

    fn unicode_escape(&mut self, output: &mut String) -> Result<()> {
        let first = self.hex4()?;
        let scalar = if (0xd800..=0xdbff).contains(&first) {
            self.take(b'\\')?;
            self.take(b'u')?;
            let second = self.hex4()?;
            if !(0xdc00..=0xdfff).contains(&second) {
                bail!("canonical JSON has an invalid surrogate pair");
            }
            0x10000 + (((first - 0xd800) as u32) << 10) + (second - 0xdc00) as u32
        } else if (0xdc00..=0xdfff).contains(&first) {
            bail!("canonical JSON has an unpaired low surrogate");
        } else {
            first as u32
        };
        output.push(char::from_u32(scalar).context("canonical JSON Unicode scalar is invalid")?);
        Ok(())
    }

    fn hex4(&mut self) -> Result<u16> {
        if self.offset + 4 > self.bytes.len() {
            bail!("canonical JSON Unicode escape is incomplete");
        }
        let value = std::str::from_utf8(&self.bytes[self.offset..self.offset + 4])?;
        self.offset += 4;
        u16::from_str_radix(value, 16).context("canonical JSON Unicode escape is invalid")
    }

    fn literal(&mut self, value: &[u8]) -> Result<()> {
        if self.bytes.get(self.offset..self.offset + value.len()) != Some(value) {
            bail!("canonical JSON literal is invalid");
        }
        self.offset += value.len();
        Ok(())
    }
    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.offset += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }
    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.offset += 1;
            true
        } else {
            false
        }
    }
    fn take(&mut self, byte: u8) -> Result<()> {
        if self.consume(byte) {
            Ok(())
        } else {
            bail!("canonical JSON delimiter is invalid")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bytes_are_order_independent_nfc_and_restart_stable() {
        let left = parse_strict_json(r#"{"z":null,"name":"e\u0301","a":[1,true]}"#).unwrap();
        let right = parse_strict_json("{\"a\":[1,true],\"name\":\"é\",\"z\":null}").unwrap();
        assert_eq!(left.to_bytes(), right.to_bytes());
        assert_eq!(
            parse_strict_json(std::str::from_utf8(&left.to_bytes()).unwrap()).unwrap(),
            left
        );
    }

    #[test]
    fn duplicate_pre_and_post_nfc_keys_fail() {
        assert!(parse_strict_json(r#"{"a":1,"a":2}"#).is_err());
        assert!(parse_strict_json("{\"é\":1,\"é\":2}").is_err());
    }

    #[test]
    fn numbers_are_integer_only_and_explicit_null_is_distinct() {
        for invalid in ["-0", "01", "1.0", "1e2", "9223372036854775808"] {
            assert!(parse_strict_json(invalid).is_err(), "accepted {invalid}");
        }
        assert_ne!(
            parse_strict_json("{}").unwrap(),
            parse_strict_json(r#"{"x":null}"#).unwrap()
        );
    }
}
