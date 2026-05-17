use anyhow::{anyhow, Result};
use std::collections::BTreeMap;

/// Parsed VDF/KeyValues node. Either a leaf string value or a nested object.
#[derive(Debug, Clone)]
pub enum VdfValue {
    String(String),
    Object(BTreeMap<String, VdfValue>),
}

impl VdfValue {
    /// Returns the string contents if this node is a leaf.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            VdfValue::String(s) => Some(s.as_str()),
            VdfValue::Object(_) => None,
        }
    }

    /// Returns the child map if this node is an object.
    pub fn as_object(&self) -> Option<&BTreeMap<String, VdfValue>> {
        match self {
            VdfValue::Object(map) => Some(map),
            VdfValue::String(_) => None,
        }
    }

    /// Looks up a child by key, ignoring ASCII case.
    pub fn get_ci(&self, key: &str) -> Option<&VdfValue> {
        let map = self.as_object()?;
        for (k, v) in map.iter() {
            if k.eq_ignore_ascii_case(key) {
                return Some(v);
            }
        }
        None
    }
}

/// Parses a VDF document and returns the wrapped root object.
pub fn parse(input: &str) -> Result<VdfValue> {
    let mut tokenizer = Tokenizer::new(input);
    let mut root = BTreeMap::new();

    loop {
        match tokenizer.next_token()? {
            Some(Token::String(key)) => {
                let value = parse_value(&mut tokenizer)?;
                root.insert(key, value);
            }
            Some(Token::OpenBrace) | Some(Token::CloseBrace) => {
                return Err(anyhow!("Unexpected brace at top level"));
            }
            None => break,
        }
    }

    Ok(VdfValue::Object(root))
}

fn parse_value(tokenizer: &mut Tokenizer<'_>) -> Result<VdfValue> {
    match tokenizer
        .next_token()?
        .ok_or_else(|| anyhow!("Unexpected end of input while reading value"))?
    {
        Token::String(s) => Ok(VdfValue::String(s)),
        Token::OpenBrace => parse_object(tokenizer),
        Token::CloseBrace => Err(anyhow!("Unexpected '}}' while reading value")),
    }
}

fn parse_object(tokenizer: &mut Tokenizer<'_>) -> Result<VdfValue> {
    let mut map = BTreeMap::new();
    loop {
        match tokenizer.next_token()? {
            Some(Token::CloseBrace) => return Ok(VdfValue::Object(map)),
            Some(Token::String(key)) => {
                let value = parse_value(tokenizer)?;
                map.insert(key, value);
            }
            Some(Token::OpenBrace) => return Err(anyhow!("Unexpected '{{' inside object")),
            None => return Err(anyhow!("Unterminated object literal")),
        }
    }
}

#[derive(Debug)]
enum Token {
    String(String),
    OpenBrace,
    CloseBrace,
}

struct Tokenizer<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>> {
        loop {
            self.skip_whitespace();
            if self.pos >= self.bytes.len() {
                return Ok(None);
            }
            let c = self.bytes[self.pos];
            if c == b'/' && self.peek(1) == Some(b'/') {
                self.skip_to_eol();
                continue;
            }
            if c == b'{' {
                self.pos += 1;
                return Ok(Some(Token::OpenBrace));
            }
            if c == b'}' {
                self.pos += 1;
                return Ok(Some(Token::CloseBrace));
            }
            if c == b'"' {
                let s = self.read_quoted_string()?;
                return Ok(Some(Token::String(s)));
            }
            let s = self.read_bare_string()?;
            return Ok(Some(Token::String(s)));
        }
    }

    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn skip_to_eol(&mut self) {
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            self.pos += 1;
            if c == b'\n' {
                break;
            }
        }
    }

    fn read_quoted_string(&mut self) -> Result<String> {
        self.pos += 1;
        let mut buf = String::new();
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            if c == b'\\' {
                self.pos += 1;
                if self.pos >= self.bytes.len() {
                    return Err(anyhow!("Unterminated escape sequence"));
                }
                let esc = self.bytes[self.pos];
                self.pos += 1;
                match esc {
                    b'\\' => buf.push('\\'),
                    b'"' => buf.push('"'),
                    b'n' => buf.push('\n'),
                    b'r' => buf.push('\r'),
                    b't' => buf.push('\t'),
                    other => buf.push(other as char),
                }
                continue;
            }
            if c == b'"' {
                self.pos += 1;
                return Ok(buf);
            }
            self.pos += 1;
            buf.push(c as char);
        }
        Err(anyhow!("Unterminated quoted string"))
    }

    fn read_bare_string(&mut self) -> Result<String> {
        let start = self.pos;
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            if c == b' '
                || c == b'\t'
                || c == b'\n'
                || c == b'\r'
                || c == b'{'
                || c == b'}'
                || c == b'"'
            {
                break;
            }
            self.pos += 1;
        }
        if start == self.pos {
            return Err(anyhow!("Empty token"));
        }
        Ok(String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_pairs() {
        let input = r#"
            "AppState"
            {
                "appid"        "228980"
                "name"         "Steamworks Common Redistributables"
                "installdir"   "Steamworks Shared"
            }
        "#;
        let parsed = parse(input).unwrap();
        let app_state = parsed.get_ci("AppState").unwrap();
        assert_eq!(app_state.get_ci("appid").and_then(|v| v.as_str()), Some("228980"));
        assert_eq!(
            app_state.get_ci("installdir").and_then(|v| v.as_str()),
            Some("Steamworks Shared")
        );
    }

    #[test]
    fn parses_nested_objects() {
        let input = r#"
            "libraryfolders"
            {
                "0"
                {
                    "path"        "C:\\Program Files (x86)\\Steam"
                    "label"       ""
                    "apps"
                    {
                        "228980"  "418176224"
                        "550"     "13283632056"
                    }
                }
                "1"
                {
                    "path"        "D:\\SteamLibrary"
                    "apps"
                    {
                        "292030"  "126268924417"
                    }
                }
            }
        "#;
        let parsed = parse(input).unwrap();
        let lf = parsed.get_ci("libraryfolders").unwrap();
        let folder0 = lf.get_ci("0").unwrap();
        assert_eq!(
            folder0.get_ci("path").and_then(|v| v.as_str()),
            Some("C:\\Program Files (x86)\\Steam")
        );
        let folder1 = lf.get_ci("1").unwrap();
        assert_eq!(
            folder1.get_ci("path").and_then(|v| v.as_str()),
            Some("D:\\SteamLibrary")
        );
        let apps1 = folder1.get_ci("apps").unwrap();
        assert_eq!(apps1.get_ci("292030").and_then(|v| v.as_str()), Some("126268924417"));
    }

    #[test]
    fn ignores_line_comments() {
        let input = r#"
            // top comment
            "root"
            {
                // inside comment
                "key" "value"
            }
        "#;
        let parsed = parse(input).unwrap();
        assert_eq!(
            parsed
                .get_ci("root")
                .and_then(|r| r.get_ci("key"))
                .and_then(|v| v.as_str()),
            Some("value")
        );
    }

    #[test]
    fn handles_escaped_characters() {
        let input = r#""root" { "path" "C:\\Games\\My Game" "quoted" "say \"hi\"" }"#;
        let parsed = parse(input).unwrap();
        let root = parsed.get_ci("root").unwrap();
        assert_eq!(
            root.get_ci("path").and_then(|v| v.as_str()),
            Some("C:\\Games\\My Game")
        );
        assert_eq!(
            root.get_ci("quoted").and_then(|v| v.as_str()),
            Some(r#"say "hi""#)
        );
    }

    #[test]
    fn rejects_unterminated_object() {
        let input = r#""root" { "key" "value" "#;
        assert!(parse(input).is_err());
    }
}
