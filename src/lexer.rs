use std::str::CharIndices;
use std::fmt;

/// Macro used to generate Keyword enum based on collection of string slices
macro_rules! keywords {
    ($($val:expr => $name:ident),*) => {
        #[derive(Copy, Clone, PartialEq)]
        pub enum Keyword { $($name,)* }

        impl fmt::Debug for Keyword {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                match self {
                    $(&Keyword::$name => write!(f, $val)?),*
                }
                Ok(())
            }
        }

        const KEYWORDS: [(&'static str, Keyword); count_idents!($($name),*)] = [$(($val, Keyword::$name)),*];
    }
}

/// Macro used to count idents separated by commas
macro_rules! count_idents {
    () => {0};
    ($ident:ident $(,$rest:ident)*) => {1 + count_idents!($($rest),*)};
}

/// Registration of keywords supported by language
keywords! {
    "if" => If,
    "else" => Else,
    "for" => For,
    "while" => While,
    "loop" => Loop,
    "func" => Func,
    "yield" => Yield,
    "return" => Return,
    "break" => Break,
    "true" => True,
    "false" => False,
    "in" => In
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Special {
    Single(char),
    #[allow(dead_code)]
    Double(char, char),
    #[allow(dead_code)]
    Triple(char, char, char),
}

/// Storage for values stored in a single token
#[derive(Clone, Debug, PartialEq)]
pub enum TokenValue {
    None,
    Special(Special),
    Identifier,
    IntegralNumber(i32),
    FloatingNumber(f32),
    String(String),
    Keyword(Keyword),
}

/// Type of the token
#[derive(Copy, Clone, PartialEq)]
pub enum TokenType {
    Special(Special),
    Identifier,
    IntegralNumber,
    FloatingNumber,
    String,
    Keyword(Keyword),
    EndOfSource,
}

impl fmt::Debug for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TokenType::Special(ch) => write!(f, "`{:?}`", ch)?,
            TokenType::Identifier => write!(f, "identifier")?,
            TokenType::IntegralNumber => write!(f, "integral literal")?,
            TokenType::FloatingNumber => write!(f, "floating literal")?,
            TokenType::String => write!(f, "string")?,
            TokenType::Keyword(keyword) => write!(f, "`{:?} keyword`", keyword)?,
            TokenType::EndOfSource => write!(f, "end of source")?,
        }
        Ok(())
    }
}

/// Lexical unit produced by lexical analysis of source code
#[derive(Clone)]
pub struct Token<'a> {
    /// Value stored in the token
    value: TokenValue,
    /// Slice of the raw source with raw representation of the token
    lexeme: Lexeme<'a>,
    /// Number of the line that the token in the source starts at
    line: usize,
    /// Number of the column that the token in the source starts at
    column: usize,
}

/// Single lexical unit of the source, eg. identifier, literals, etc
impl<'a> Token<'a> {
    /// Returns raw slice of the input that represents the token in the source
    pub fn get_lexeme(&self) -> Lexeme<'a> {
        self.lexeme
    }

    /// Returns line number at which exists the first char of the token
    pub fn get_line(&self) -> usize {
        self.line
    }

    /// Returns column number at which exists the first char of the token
    pub fn get_column(&self) -> usize {
        self.column
    }

    /// Returns type of the token
    pub fn get_type(&self) -> TokenType {
        match self.value {
            TokenValue::Special(special) => TokenType::Special(special),
            TokenValue::Identifier => TokenType::Identifier,
            TokenValue::Keyword(kw) => TokenType::Keyword(kw),
            TokenValue::IntegralNumber(_) => TokenType::IntegralNumber,
            TokenValue::FloatingNumber(_) => TokenType::FloatingNumber,
            TokenValue::None => TokenType::EndOfSource,
            TokenValue::String(_) => TokenType::String,
        }
    }

    /// Returns the char that is representing the token when it is a special
    pub fn get_special(&self) -> Option<Special> {
        match self.value {
            TokenValue::Special(special) => Some(special),
            _ => None
        }
    }

    /// Returns the integral number when token is a integer literal
    pub fn get_integer(&self) -> Option<i32> {
        match self.value {
            TokenValue::IntegralNumber(val) => Some(val),
            _ => None
        }
    }

    /// Returns the float number when token is a float literal
    pub fn get_float(&self) -> Option<f32> {
        match self.value {
            TokenValue::FloatingNumber(val) => Some(val),
            _ => None
        }
    }

    /// Returns a raw slice over the meaningful string value of the token
    pub fn as_slice(&'a self) -> &'a str {
        match self.value {
            TokenValue::String(ref s) => &s[..],
            _ => self.get_lexeme().as_slice(),
        }
    }

    /// Returns an owned string over the meaningful string value of the token
    pub fn as_string(&self) -> String {
        self.as_slice().to_owned()
    }
}

impl<'a> fmt::Debug for Token<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} at {}:{}", self.value, self.line, self.column)
    }
}

/// Structure being context for lexical analysis
pub struct Lexer<'a> {
    /// Source to parse
    source: &'a str,
    /// Line number starting from 1 at which starts current token
    line: usize,
    /// Column number starting from 1 at which starts current token
    column: usize,
    /// Current position in the source (in bytes, not codepoints)
    position: usize,
    /// Iterator over the chars of the source
    iter: CharIndices<'a>,
    /// Recently peeked character with its position in the source (in bytes, not codepoints)
    peeked: Option<(usize, char)>,
    /// Size of the tab in number of spaces
    tab_size: u8,
}

impl<'a> Lexer<'a> {
    /// Returns lexer created from given source
    pub fn from_source(input: &'a str) -> Lexer<'a> {
        Lexer {
            source: input,
            line: 1,
            column: 1,
            position: 0,
            iter: input.char_indices(),
            peeked: None,
            tab_size: 4,
        }
    }
}

/// Custom slice that holds lexeme extracted from source
#[derive(Copy, Clone)]
pub struct Lexeme<'a> {
    /// Starting position of the lexeme in the source
    start: usize,
    /// Length of the lexeme (in bytes)
    length: usize,
    /// Source that lexeme refers to
    raw: &'a str,
}

impl<'a> Lexeme<'a> {
    /// Returns plain slice of the source
    pub fn as_slice(&self) -> &'a str {
        &self.raw[self.start..self.start + self.length]
    }

    /// Returns lexeme created from a str
    pub fn from_str(str: &'a str) -> Lexeme<'a> {
        Lexeme {
            raw: str,
            start: 0,
            length: str.bytes().len(),
        }
    }

    /// Checks whether current lexeme directly adjoins with the other
    #[allow(dead_code)]
    pub fn adjoins_with(&self, other: &Lexeme<'a>) -> bool {
        self.raw.as_ptr() == other.raw.as_ptr() && other.start - self.start == self.length
    }
}

impl<'a> fmt::Debug for Lexeme<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &self.raw[self.start..self.start + self.length])
    }
}

impl<'a> Default for Lexeme<'a> {
    fn default() -> Lexeme<'a> {
        Lexeme::from_str("")
    }
}

/// Error returned by lexer
#[derive(Debug, PartialEq)]
pub enum LexerError {
    UnexpectedEndOfSource(usize, usize),
}

pub type LexerResult<T> = Result<T, LexerError>;

impl<'a> Lexer<'a> {
    /// Returns next token from the source
    pub fn next(&mut self) -> LexerResult<Token<'a>> {
        self.skip_space();
        let token = match self.peek() {
            Some(ch) if self.can_start_identifier(ch) => self.match_keyword_or_identifier()?,
            Some(ch) if ch.is_digit(10) => self.match_number()?,
            Some('"') => self.match_string()?,
            Some(ch) => self.match_special(ch)?,
            None => self.match_end_of_source()?,
        };
        Ok(token)
    }

    /// Checks whether given character can be a starting character of the identifier
    #[inline]
    fn can_start_identifier(&self, ch: char) -> bool {
        match ch {
            _ if ch.is_alphabetic() => true,
            '_' => true,
            _ => false,
        }
    }

    /// Checks whether given character can be in the identifier
    #[inline]
    fn can_be_in_identifier(&self, ch: char) -> bool {
        match ch {
            _ if ch.is_alphanumeric() => true,
            '_' => true,
            _ => false,
        }
    }

    /// Skips all whitespaces
    fn skip_space(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance().unwrap();
            } else {
                break;
            }
        }
    }

    /// Returns current token when it is a keyword or an identifier
    fn match_keyword_or_identifier(&mut self) -> LexerResult<Token<'a>> {
        let (line, column) = (self.line, self.column);
        let idx_start = self.position;
        while let Some(ch) = self.peek() {
            if self.can_be_in_identifier(ch) {
                self.advance().unwrap();
            } else {
                break;
            }
        }
        let lexeme = self.take_slice_from(idx_start);
        let kind = match self.get_keyword(lexeme.as_slice()) {
            Some(keyword) => TokenValue::Keyword(keyword),
            None => TokenValue::Identifier,
        };
        Ok(Token { value: kind, lexeme, line, column })
    }

    /// Returns current token when it is a string literal
    fn match_string(&mut self) -> LexerResult<Token<'a>> {
        let (line, column) = (self.line, self.column);
        let idx_start = self.position;
        // '"'
        self.advance().unwrap();
        let mut string = String::new();
        loop {
            match self.peek() {
                Some(ch) => match ch {
                    '"' => {
                        self.advance().unwrap();
                        break;
                    }
                    ch => {
                        string.push(ch);
                        self.advance().unwrap();
                    }
                },
                None => return Err(LexerError::UnexpectedEndOfSource(self.line, self.column)),
            }
        }
        Ok(Token {
            value: TokenValue::String(string),
            lexeme: self.take_slice_from(idx_start),
            line,
            column,
        })
    }

    /// Returns slice of the source starting from a given index and ending at current index
    fn take_slice_from(&mut self, idx_start: usize) -> Lexeme<'a> {
        let idx_end = self.peek_index().unwrap_or(self.source.len());
        Lexeme { start: idx_start, length: idx_end - idx_start, raw: self.source }
    }

    /// Returns current token when it is built of one or more special chars
    fn match_special(&mut self, first: char) -> LexerResult<Token<'a>> {
        let (line, column) = (self.line, self.column);
        let lexeme = {
            let start_idx = self.position;
            self.advance().unwrap();
            self.take_slice_from(start_idx)
        };
        Ok(Token {
            value: TokenValue::Special(Special::Single(first)),
            lexeme,
            line,
            column,
        })
    }

    fn match_end_of_source(&mut self) -> LexerResult<Token<'a>> {
        Ok(Token {
            value: TokenValue::None,
            lexeme: Lexeme::default(),
            line: self.line,
            column: self.column,
        })
    }

    /// Returns current token when it is a number
    fn match_number(&mut self) -> LexerResult<Token<'a>> {
        let (line, column) = (self.line, self.column);
        let idx_start = self.position;
        let mut is_floating = false;
        self.advance_while_digits();
        if let Some('.') = self.peek() {
            self.advance().unwrap();
            self.advance_while_digits();
            is_floating = true;
        }
        let has_exponent = match self.peek() {
            Some('e') | Some('E') => {
                self.advance().unwrap();
                true
            }
            _ => false
        };
        if has_exponent {
            match self.peek() {
                Some('+') | Some('-') => {
                    self.advance().unwrap();
                }
                _ => (),
            }
            self.advance_while_digits();
            is_floating = true;
        };

        let lexeme = self.take_slice_from(idx_start);
        let value = if is_floating {
            let parsed = lexeme.as_slice().parse::<f32>().unwrap();
            TokenValue::FloatingNumber(parsed)
        } else {
            let parsed = lexeme.as_slice().parse::<i32>().unwrap();
            TokenValue::IntegralNumber(parsed)
        };

        Ok(Token { value, lexeme, line, column })
    }

    fn advance_while_digits(&mut self) {
        loop {
            match self.peek() {
                Some('0'...'9') | Some('_') => self.advance().unwrap(),
                _ => break,
            };
        }
    }

    /// Returns a keyword when a given text can be one
    fn get_keyword(&mut self, text: &str) -> Option<Keyword> {
        KEYWORDS
            .iter()
            .filter(|&&(lex, _val)| lex == text)
            .map(|&(_lex, val)| val)
            .next()
    }

    /// Returns current character without advancing the iterator
    #[inline]
    fn peek(&mut self) -> Option<char> {
        self.ensure_peeked();
        self.peeked.map(|x| x.1)
    }

    /// Returns current character index without advancing the iterator
    #[inline]
    fn peek_index(&mut self) -> Option<usize> {
        self.ensure_peeked();
        self.peeked.map(|x| x.0)
    }

    /// Loads next character when there is one
    #[inline]
    fn ensure_peeked(&mut self) {
        if self.peeked.is_none() {
            self.peeked = self.iter.next();
        }
    }

    // Advances the iterator and returns consumed character
    fn advance(&mut self) -> Option<char> {
        self.peek()?;
        let (_idx, ch) = self.peeked.take()?;
        match ch {
            '\n' => {
                self.column = 1;
                self.line += 1;
            }
            '\t' => {
                self.column += self.tab_size as usize;
            }
            _ => {
                self.column += 1;
            }
        }
        if let Some(idx) = self.peek_index() {
            self.position = idx;
        }
        Some(ch)
    }
}