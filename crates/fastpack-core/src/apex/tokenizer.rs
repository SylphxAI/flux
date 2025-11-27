//! JSON Tokenizer
//!
//! Fast, zero-copy JSON tokenization for structure extraction.

/// JSON Token types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    /// Object start `{`
    ObjectStart,
    /// Object end `}`
    ObjectEnd,
    /// Array start `[`
    ArrayStart,
    /// Array end `]`
    ArrayEnd,
    /// String value (start position, length)
    String(usize, usize),
    /// Number value (start position, length)
    Number(usize, usize),
    /// Boolean true
    True,
    /// Boolean false
    False,
    /// Null
    Null,
    /// Colon `:`
    Colon,
    /// Comma `,`
    Comma,
}

/// Fast JSON tokenizer
pub struct Tokenizer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    /// Get next token
    pub fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();

        if self.pos >= self.input.len() {
            return None;
        }

        let byte = self.input[self.pos];
        let token = match byte {
            b'{' => {
                self.pos += 1;
                Token::ObjectStart
            }
            b'}' => {
                self.pos += 1;
                Token::ObjectEnd
            }
            b'[' => {
                self.pos += 1;
                Token::ArrayStart
            }
            b']' => {
                self.pos += 1;
                Token::ArrayEnd
            }
            b':' => {
                self.pos += 1;
                Token::Colon
            }
            b',' => {
                self.pos += 1;
                Token::Comma
            }
            b'"' => self.read_string(),
            b't' => self.read_true(),
            b'f' => self.read_false(),
            b'n' => self.read_null(),
            b'-' | b'0'..=b'9' => self.read_number(),
            _ => return None,
        };

        Some(token)
    }

    /// Tokenize entire input
    pub fn tokenize_all(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while let Some(token) = self.next_token() {
            tokens.push(token);
        }
        tokens
    }

    /// Get slice of input at position
    pub fn slice(&self, start: usize, len: usize) -> &'a [u8] {
        &self.input[start..start + len]
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn read_string(&mut self) -> Token {
        let start = self.pos + 1; // Skip opening quote
        self.pos += 1;

        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b'"' => {
                    let len = self.pos - start;
                    self.pos += 1; // Skip closing quote
                    return Token::String(start, len);
                }
                b'\\' => {
                    self.pos += 2; // Skip escape sequence
                }
                _ => self.pos += 1,
            }
        }

        Token::String(start, self.pos - start)
    }

    fn read_number(&mut self) -> Token {
        let start = self.pos;

        // Optional minus
        if self.pos < self.input.len() && self.input[self.pos] == b'-' {
            self.pos += 1;
        }

        // Integer part
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
            self.pos += 1;
        }

        // Fractional part
        if self.pos < self.input.len() && self.input[self.pos] == b'.' {
            self.pos += 1;
            while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }

        // Exponent
        if self.pos < self.input.len() && (self.input[self.pos] == b'e' || self.input[self.pos] == b'E') {
            self.pos += 1;
            if self.pos < self.input.len() && (self.input[self.pos] == b'+' || self.input[self.pos] == b'-') {
                self.pos += 1;
            }
            while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }

        Token::Number(start, self.pos - start)
    }

    fn read_true(&mut self) -> Token {
        self.pos += 4; // "true"
        Token::True
    }

    fn read_false(&mut self) -> Token {
        self.pos += 5; // "false"
        Token::False
    }

    fn read_null(&mut self) -> Token {
        self.pos += 4; // "null"
        Token::Null
    }
}

/// Check if input looks like JSON
pub fn is_json(input: &[u8]) -> bool {
    let mut i = 0;
    // Skip whitespace
    while i < input.len() && matches!(input[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    // Check for JSON start
    i < input.len() && matches!(input[i], b'{' | b'[')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_object() {
        let input = br#"{"name":"test","value":123}"#;
        let mut tokenizer = Tokenizer::new(input);
        let tokens = tokenizer.tokenize_all();

        assert_eq!(tokens.len(), 9);
        assert_eq!(tokens[0], Token::ObjectStart);
        assert!(matches!(tokens[1], Token::String(_, _)));
        assert_eq!(tokens[2], Token::Colon);
        assert!(matches!(tokens[3], Token::String(_, _)));
        assert_eq!(tokens[4], Token::Comma);
        assert!(matches!(tokens[5], Token::String(_, _)));
        assert_eq!(tokens[6], Token::Colon);
        assert!(matches!(tokens[7], Token::Number(_, _)));
        assert_eq!(tokens[8], Token::ObjectEnd);
    }

    #[test]
    fn test_tokenize_array() {
        let input = b"[1, 2, 3]";
        let mut tokenizer = Tokenizer::new(input);
        let tokens = tokenizer.tokenize_all();

        assert_eq!(tokens.len(), 7);
        assert_eq!(tokens[0], Token::ArrayStart);
        assert_eq!(tokens[6], Token::ArrayEnd);
    }

    #[test]
    fn test_tokenize_nested() {
        let input = br#"{"arr":[1,2],"obj":{"x":true}}"#;
        let mut tokenizer = Tokenizer::new(input);
        let tokens = tokenizer.tokenize_all();

        assert!(tokens.len() > 10);
    }

    #[test]
    fn test_is_json() {
        assert!(is_json(b"{}"));
        assert!(is_json(b"[]"));
        assert!(is_json(b"  {\"a\":1}"));
        assert!(!is_json(b"hello"));
        assert!(!is_json(b"123"));
    }

    #[test]
    fn test_string_extraction() {
        let input = br#"{"key":"value"}"#;
        let mut tokenizer = Tokenizer::new(input);
        let tokens = tokenizer.tokenize_all();

        if let Token::String(start, len) = tokens[1] {
            assert_eq!(tokenizer.slice(start, len), b"key");
        } else {
            panic!("Expected string token");
        }

        if let Token::String(start, len) = tokens[3] {
            assert_eq!(tokenizer.slice(start, len), b"value");
        } else {
            panic!("Expected string token");
        }
    }
}
