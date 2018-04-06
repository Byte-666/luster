use std::char;
use std::collections::HashMap;
use std::io::Read;

use failure::{err_msg, Error, ResultExt};

use stream::Stream;

#[derive(Debug, Clone, PartialEq)]
pub enum Token<'a> {
    Break,
    Do,
    Else,
    ElseIf,
    End,
    Function,
    Goto,
    If,
    In,
    Local,
    Nil,
    For,
    While,
    Repeat,
    Until,
    Return,
    Then,
    True,
    False,
    Not,
    And,
    Or,
    Plus,
    Minus,
    Times,
    Div,
    IDiv,
    Pow,
    Mod,
    BitAnd,
    BitNot,
    BitOr,
    ShiftRight,
    ShiftLeft,
    Dot,
    Concat,
    Dots,
    Assign,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    Colon,
    DoubleColon,
    Len,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Number(f64),
    Integer(i64),
    Name(&'a [u8]),
    String(&'a [u8]),
}

pub struct Lexer<R: Read> {
    lexer_state: LexerState<R>,
    output_buffer: Vec<u8>,
}

#[derive(Debug)]
pub enum NextToken<'a> {
    Next {
        token: Token<'a>,
        /// Line number that the token *begins* on, 0-indexed
        line_number: u64,
    },
    End,
}

impl<R: Read> Lexer<R> {
    pub fn new(source: R) -> Lexer<R> {
        Lexer {
            lexer_state: LexerState {
                stream: Stream::new(source),
                line_number: 0,
            },
            output_buffer: Vec::new(),
        }
    }

    pub fn next<'a>(&'a mut self) -> Result<NextToken<'a>, Error> {
        self.lexer_state.skip_whitespace()?;
        let line_number = self.lexer_state.line_number;
        self.output_buffer.clear();

        if let Some(token) = self.lexer_state
            .lex(&mut self.output_buffer)
            .with_context(|_| format!("starting at line number: {}", line_number))?
        {
            return Ok(NextToken::Next { token, line_number });
        } else {
            return Ok(NextToken::End);
        }
    }
}

struct LexerState<R: Read> {
    stream: Stream<R>,
    line_number: u64,
}

impl<R: Read> LexerState<R> {
    fn skip_whitespace(&mut self) -> Result<(), Error> {
        while let Some(c) = self.stream.peek(0)? {
            match c {
                SPACE | HORIZONTAL_TAB | VERTICAL_TAB | FORM_FEED => {
                    self.stream.advance(1)?;
                }

                NEWLINE | CARRIAGE_RETURN => {
                    self.line_end(None)?;
                }

                MINUS => {
                    if self.stream.peek(1)? != Some(MINUS) {
                        break;
                    } else {
                        self.comment()?;
                    }
                }

                _ => break,
            }
        }

        Ok(())
    }

    fn lex<'a>(&mut self, output_buffer: &'a mut Vec<u8>) -> Result<Option<Token<'a>>, Error> {
        let c = if let Some(c) = self.stream.peek(0)? {
            c
        } else {
            return Ok(None);
        };

        Ok(match c {
            SPACE | HORIZONTAL_TAB | VERTICAL_TAB | FORM_FEED => {
                self.stream.advance(1)?;
                None
            }

            NEWLINE | CARRIAGE_RETURN => {
                self.line_end(None)?;
                None
            }

            MINUS => {
                if self.stream.peek(1)? != Some(MINUS) {
                    self.stream.advance(1)?;
                    Some(Token::Minus)
                } else {
                    self.comment()?;
                    None
                }
            }

            LEFT_BRACKET => {
                let next = self.stream.peek(1)?;
                if next == Some(EQUALS) || next == Some(LEFT_BRACKET) {
                    self.long_string(Some(output_buffer))?;
                    Some(Token::String(output_buffer.as_slice()))
                } else {
                    self.stream.advance(1)?;
                    Some(Token::LeftBracket)
                }
            }

            EQUALS => {
                self.stream.advance(1)?;
                if self.stream.peek(0)? == Some(EQUALS) {
                    self.stream.advance(1)?;
                    Some(Token::Equal)
                } else {
                    Some(Token::Assign)
                }
            }

            LESS_THAN => {
                self.stream.advance(1)?;
                let next = self.stream.peek(0)?;
                if next == Some(EQUALS) {
                    self.stream.advance(1)?;
                    Some(Token::LessEqual)
                } else if next == Some(LESS_THAN) {
                    self.stream.advance(1)?;
                    Some(Token::ShiftLeft)
                } else {
                    Some(Token::Less)
                }
            }

            GREATER_THAN => {
                self.stream.advance(1)?;
                let next = self.stream.peek(0)?;
                if next == Some(EQUALS) {
                    self.stream.advance(1)?;
                    Some(Token::GreaterEqual)
                } else if next == Some(GREATER_THAN) {
                    self.stream.advance(1)?;
                    Some(Token::ShiftRight)
                } else {
                    Some(Token::Greater)
                }
            }

            FORWARD_SLASH => {
                self.stream.advance(1)?;
                if self.stream.peek(0)? == Some(FORWARD_SLASH) {
                    self.stream.advance(1)?;
                    Some(Token::IDiv)
                } else {
                    Some(Token::Div)
                }
            }

            TILDE => {
                self.stream.advance(1)?;
                if self.stream.peek(0)? == Some(EQUALS) {
                    self.stream.advance(1)?;
                    Some(Token::NotEqual)
                } else {
                    Some(Token::BitNot)
                }
            }

            COLON => {
                self.stream.advance(1)?;
                if self.stream.peek(0)? == Some(COLON) {
                    self.stream.advance(1)?;
                    Some(Token::DoubleColon)
                } else {
                    Some(Token::Colon)
                }
            }

            DOUBLE_QUOTE | SINGLE_QUOTE => {
                self.short_string(output_buffer)?;
                Some(Token::String(output_buffer.as_slice()))
            }

            PERIOD => {
                if self.stream.peek(1)? == Some(PERIOD) {
                    if self.stream.peek(2)? == Some(PERIOD) {
                        self.stream.advance(3)?;
                        Some(Token::Dots)
                    } else {
                        self.stream.advance(2)?;
                        Some(Token::Concat)
                    }
                } else {
                    if self.stream.peek(1)?.map(|c| is_digit(c)).unwrap_or(false) {
                        Some(match self.numeral()? {
                            Numeral::Integer(i) => Token::Integer(i),
                            Numeral::Float(f) => Token::Number(f),
                        })
                    } else {
                        Some(Token::Dot)
                    }
                }
            }

            c => {
                if is_digit(c) {
                    Some(match self.numeral()? {
                        Numeral::Integer(i) => Token::Integer(i),
                        Numeral::Float(f) => Token::Number(f),
                    })
                } else if let Some(t) = SINGLE_CHAR_TOKEN_MAP.get(&c).cloned() {
                    self.stream.advance(1)?;
                    Some(t)
                } else if is_alpha(c) {
                    output_buffer.push(c);
                    self.stream.advance(1)?;

                    while let Some(c) = self.stream.peek(0)? {
                        if is_alpha(c) || is_digit(c) {
                            output_buffer.push(c);
                            self.stream.advance(1)?;
                        } else {
                            break;
                        }
                    }

                    if let Some(t) = RESERVED_WORD_MAP.get(output_buffer.as_slice()).cloned() {
                        Some(t)
                    } else {
                        Some(Token::Name(output_buffer.as_slice()))
                    }
                } else {
                    return Err(format_err!(
                        "unexpected character: '{}'",
                        char::from_u32(c as u32).unwrap_or(char::REPLACEMENT_CHARACTER)
                    ));
                }
            }
        })
    }

    // Read any of "\n", "\r", "\n\r", or "\r\n" as a single newline, and increment the current line
    // number.
    fn line_end(&mut self, mut output: Option<&mut Vec<u8>>) -> Result<(), Error> {
        let newline = self.stream.peek(0).unwrap().unwrap();
        assert!(is_newline(newline));
        self.stream.advance(1)?;
        if let Some(o) = output.as_mut() {
            o.push(newline);
        }

        if let Some(next_newline) = self.stream.peek(0)? {
            if is_newline(next_newline) && next_newline != newline {
                self.stream.advance(1)?;
                if let Some(o) = output.as_mut() {
                    o.push(next_newline);
                }
            }
        }

        self.line_number += 1;
        Ok(())
    }

    // Read a string on a single line delimited by ' or " that allows for \ escaping of certain
    // characters.
    fn short_string(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let start_quote = self.stream.peek(0).unwrap().unwrap();
        assert!(start_quote == SINGLE_QUOTE || start_quote == DOUBLE_QUOTE);
        self.stream.advance(1)?;

        loop {
            let c = if let Some(c) = self.stream.peek(0)? {
                c
            } else {
                return Err(err_msg("unfinished short string"));
            };

            if is_newline(c) {
                return Err(err_msg("unfinished short string"));
            }

            self.stream.advance(1)?;
            if c == BACKSLASH {
                match self.stream
                    .peek(0)?
                    .ok_or_else(|| err_msg("unfinished short string"))?
                {
                    LOWER_A => {
                        self.stream.advance(1)?;
                        output.push(ALERT_BEEP);
                    }

                    LOWER_B => {
                        self.stream.advance(1)?;
                        output.push(BACKSPACE);
                    }

                    LOWER_F => {
                        self.stream.advance(1)?;
                        output.push(FORM_FEED);
                    }

                    LOWER_N => {
                        self.stream.advance(1)?;
                        output.push(NEWLINE);
                    }

                    LOWER_R => {
                        self.stream.advance(1)?;
                        output.push(CARRIAGE_RETURN);
                    }

                    LOWER_T => {
                        self.stream.advance(1)?;
                        output.push(HORIZONTAL_TAB);
                    }

                    LOWER_V => {
                        self.stream.advance(1)?;
                        output.push(VERTICAL_TAB);
                    }

                    BACKSLASH => {
                        self.stream.advance(1)?;
                        output.push(BACKSLASH);
                    }

                    SINGLE_QUOTE => {
                        self.stream.advance(1)?;
                        output.push(SINGLE_QUOTE);
                    }

                    DOUBLE_QUOTE => {
                        self.stream.advance(1)?;
                        output.push(DOUBLE_QUOTE);
                    }

                    NEWLINE | CARRIAGE_RETURN => {
                        self.line_end(Some(output))?;
                    }

                    LOWER_X => {
                        self.stream.advance(1)?;
                        let first = self.stream
                            .peek(0)?
                            .and_then(from_hex_digit)
                            .ok_or_else(|| err_msg("hexadecimal digit expected"))?;
                        let second = self.stream
                            .peek(1)?
                            .and_then(from_hex_digit)
                            .ok_or_else(|| err_msg("hexadecimal digit expected"))?;
                        output.push(first << 4 | second);
                        self.stream.advance(2)?;
                    }

                    LOWER_U => {
                        if self.stream.peek(1)? != Some(LEFT_BRACE) {
                            return Err(err_msg("missing '{'"));
                        }
                        self.stream.advance(2)?;

                        let mut u: u32 = 0;
                        loop {
                            if let Some(c) = self.stream.peek(0)? {
                                if c == RIGHT_BRACE {
                                    self.stream.advance(1)?;
                                    break;
                                } else if let Some(h) = from_hex_digit(c) {
                                    u = (u << 4) | h as u32;
                                    self.stream.advance(1)?;
                                } else {
                                    return Err(err_msg("missing '}'"));
                                }
                            } else {
                                return Err(err_msg("missing '}'"));
                            }
                        }

                        let c = char::from_u32(u).ok_or_else(|| err_msg("invalid UTF-8 value"))?;
                        let mut buf = [0; 4];
                        for &b in c.encode_utf8(&mut buf).as_bytes() {
                            output.push(b);
                        }
                    }

                    LOWER_Z => {
                        self.stream.advance(1)?;
                        while let Some(c) = self.stream.peek(0)? {
                            if is_newline(c) {
                                self.line_end(None)?;
                            } else if is_space(c) {
                                self.stream.advance(1)?;
                            } else {
                                break;
                            }
                        }
                    }

                    c => {
                        if is_digit(c) {
                            let mut u: u16 = 0;
                            for _ in 0..3 {
                                if let Some(d) = self.stream.peek(0)?.and_then(from_digit) {
                                    u = 10 * u + d as u16;
                                    self.stream.advance(1)?;
                                } else {
                                    break;
                                }
                            }
                            if u > 255 {
                                return Err(err_msg("decimal escape too large"));
                            }

                            output.push(u as u8);
                        } else {
                            return Err(err_msg("invalid escape sequence"));
                        }
                    }
                }
            } else if c == start_quote {
                break;
            } else {
                output.push(c);
            }
        }

        Ok(())
    }

    // Read a [=*[...]=*] sequence with matching numbers of '='.
    fn long_string(&mut self, mut output: Option<&mut Vec<u8>>) -> Result<(), Error> {
        assert_eq!(self.stream.peek(0).unwrap().unwrap(), LEFT_BRACKET);
        self.stream.advance(1)?;

        let mut open_sep_length = 0;
        while self.stream.peek(0)? == Some(EQUALS) {
            self.stream.advance(1)?;
            open_sep_length += 1;
        }

        if self.stream.peek(0)? != Some(LEFT_BRACKET) {
            return Err(err_msg("invalid long string delimiter"));
        }
        self.stream.advance(1)?;

        loop {
            let c = if let Some(c) = self.stream.peek(0)? {
                c
            } else {
                return Err(err_msg("unfinished long string"));
            };

            match c {
                NEWLINE | CARRIAGE_RETURN => {
                    self.line_end(output.as_mut().map(|o| &mut **o))?;
                }

                RIGHT_BRACKET => {
                    let mut close_sep_length = 0;
                    self.stream.advance(1)?;
                    while self.stream.peek(0)? == Some(EQUALS) {
                        self.stream.advance(1)?;
                        close_sep_length += 1;
                    }

                    if open_sep_length == close_sep_length
                        && self.stream.peek(0)? == Some(RIGHT_BRACKET)
                    {
                        self.stream.advance(1)?;
                        break;
                    } else {
                        // If it turns out this is not a valid long string close delimiter, we need
                        // to add the invalid close delimiter to the output.
                        if let Some(o) = output.as_mut() {
                            o.push(RIGHT_BRACKET);
                            for _ in 0..close_sep_length {
                                o.push(EQUALS);
                            }
                        }
                    }
                }

                c => {
                    if let Some(o) = output.as_mut() {
                        o.push(c);
                    }
                    self.stream.advance(1)?;
                }
            }
        }

        Ok(())
    }

    // Read either a short or long comment, either `--` to end of line, or `--` followed by a long string.
    fn comment(&mut self) -> Result<(), Error> {
        assert_eq!(self.stream.peek(0).unwrap().unwrap(), MINUS);
        assert_eq!(self.stream.peek(1).unwrap().unwrap(), MINUS);

        self.stream.advance(2)?;

        if self.stream.peek(0)? == Some(LEFT_BRACKET) {
            // long comment
            self.long_string(None)?;
        } else {
            // Short comment, read until end of line
            while let Some(c) = self.stream.peek(0)? {
                if is_newline(c) {
                    break;
                } else {
                    self.stream.advance(1)?;
                }
            }
        }

        Ok(())
    }

    fn numeral(&mut self) -> Result<Numeral, Error> {
        unimplemented!()
    }
}

enum Numeral {
    Integer(i64),
    Float(f64),
}

const SPACE: u8 = ' ' as u8;
const NEWLINE: u8 = '\n' as u8;
const CARRIAGE_RETURN: u8 = '\r' as u8;
const HORIZONTAL_TAB: u8 = '\t' as u8;
const ALERT_BEEP: u8 = 0x07;
const BACKSPACE: u8 = 0x08;
const VERTICAL_TAB: u8 = 0x0b;
const FORM_FEED: u8 = 0x0c;
const LEFT_BRACKET: u8 = '[' as u8;
const RIGHT_BRACKET: u8 = ']' as u8;
const LEFT_BRACE: u8 = '{' as u8;
const RIGHT_BRACE: u8 = '}' as u8;
const EQUALS: u8 = '=' as u8;
const LESS_THAN: u8 = '<' as u8;
const GREATER_THAN: u8 = '>' as u8;
const FORWARD_SLASH: u8 = '/' as u8;
const BACKSLASH: u8 = '\\' as u8;
const TILDE: u8 = '~' as u8;
const COLON: u8 = ':' as u8;
const DOUBLE_QUOTE: u8 = '"' as u8;
const SINGLE_QUOTE: u8 = '\'' as u8;
const PERIOD: u8 = '.' as u8;
const PLUS: u8 = '+' as u8;
const MINUS: u8 = '-' as u8;
const ASTERISK: u8 = '*' as u8;
const CARET: u8 = '^' as u8;
const PERCENT: u8 = '%' as u8;
const AMPERSAND: u8 = '&' as u8;
const OCTOTHORPE: u8 = '#' as u8;
const PIPE: u8 = '|' as u8;
const UNDERSCORE: u8 = '_' as u8;
const LOWER_A: u8 = 'a' as u8;
const LOWER_B: u8 = 'b' as u8;
const LOWER_F: u8 = 'f' as u8;
const LOWER_N: u8 = 'n' as u8;
const LOWER_R: u8 = 'r' as u8;
const LOWER_T: u8 = 't' as u8;
const LOWER_U: u8 = 'u' as u8;
const LOWER_V: u8 = 'v' as u8;
const LOWER_X: u8 = 'x' as u8;
const LOWER_Z: u8 = 'z' as u8;
const UPPER_A: u8 = 'A' as u8;
const UPPER_F: u8 = 'F' as u8;
const UPPER_Z: u8 = 'Z' as u8;
const NUM_0: u8 = '0' as u8;
const NUM_9: u8 = '9' as u8;

// Tokens that are a single character and never the beginning of another longer token
const SINGLE_CHAR_TOKENS: &[(u8, Token)] = &[
    (PLUS, Token::Plus),
    (MINUS, Token::Minus),
    (ASTERISK, Token::Times),
    (CARET, Token::Pow),
    (PERCENT, Token::Mod),
    (AMPERSAND, Token::BitAnd),
    (PIPE, Token::BitOr),
    (OCTOTHORPE, Token::Len),
    (RIGHT_BRACKET, Token::RightBracket),
    (LEFT_BRACE, Token::LeftBrace),
    (RIGHT_BRACE, Token::RightBrace),
];

// Identifiers which should be interpreted as keyword tokens
const RESERVED_WORDS: &[(&str, Token)] = &[
    ("break", Token::Break),
    ("do", Token::Do),
    ("else", Token::Else),
    ("elseif", Token::ElseIf),
    ("end", Token::End),
    ("function", Token::Function),
    ("goto", Token::Goto),
    ("if", Token::If),
    ("in", Token::In),
    ("local", Token::Local),
    ("nil", Token::Nil),
    ("for", Token::For),
    ("while", Token::While),
    ("repeat", Token::Repeat),
    ("until", Token::Until),
    ("return", Token::Return),
    ("then", Token::Then),
    ("true", Token::True),
    ("false", Token::False),
    ("not", Token::Not),
    ("and", Token::And),
    ("or", Token::Or),
];

lazy_static! {
    static ref SINGLE_CHAR_TOKEN_MAP: HashMap<u8, Token<'static>> = {
        let mut m = HashMap::new();
        for &(c, ref t) in SINGLE_CHAR_TOKENS {
            m.insert(c, t.clone());
        }
        m
    };

    static ref RESERVED_WORD_MAP: HashMap<&'static [u8], Token<'static>> = {
        let mut m = HashMap::new();
        for &(n, ref t) in RESERVED_WORDS {
            m.insert(n.as_bytes(), t.clone());
        }
        m
    };
}

fn is_newline(c: u8) -> bool {
    c == NEWLINE || c == CARRIAGE_RETURN
}

fn is_space(c: u8) -> bool {
    c == SPACE || c == HORIZONTAL_TAB || c == VERTICAL_TAB || c == FORM_FEED || is_newline(c)
}

// Is this character a lua alpha, which is A-Z, a-z, and _
fn is_alpha(c: u8) -> bool {
    (c >= LOWER_A && c <= LOWER_Z) || (c >= UPPER_A && c <= UPPER_Z) || c == UNDERSCORE
}

fn from_digit(c: u8) -> Option<u8> {
    if c >= NUM_0 && c <= NUM_9 {
        Some(c - NUM_0)
    } else {
        None
    }
}

fn is_digit(c: u8) -> bool {
    from_digit(c).is_some()
}

fn from_hex_digit(c: u8) -> Option<u8> {
    if c >= NUM_0 && c <= NUM_9 {
        Some(c - NUM_0)
    } else if c >= LOWER_A && c <= LOWER_F {
        Some(10 + c - LOWER_A)
    } else if c >= UPPER_A && c <= UPPER_F {
        Some(10 + c - UPPER_A)
    } else {
        None
    }
}