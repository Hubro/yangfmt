//
// Simple lexer to break up a stream of characters into a small set of tokens for further
// processing:
//
// - String: Any single- or double quoted string
// - Date: NNNN-NN-NN
// - Number: Any "integer-value" or "decimal-value" from the ABNF grammar
// - Comment: Any single-line comment or block comment
// - OpenCurlyBrace
// - ClosingCurlyBrace
// - SemiColon
// - Other: Any other token, including keywords, numbers, booleans and unquoted strings
//

use std::str;

use regex::Regex;

const TAB: u8 = 9;
const NEWLINE: u8 = 10;
const CARRIAGE_RETURN: u8 = 10;
const SPACE: u8 = 32;
const DOUBLE_QUOTE: u8 = 34;
const SINGLE_QUOTE: u8 = 39;
const ASTERISK: u8 = 42;
const PLUS: u8 = 43;
const DASH: u8 = 45;
const SLASH: u8 = 47;
const SEMICOLON: u8 = 59;
const BACKSLASH: u8 = 92;
const LEFT_CURLY_BRACKET: u8 = 123;
const RIGHT_CURLY_BRACKET: u8 = 125;

lazy_static! {
    static ref NUMBER_PATTERN: Regex = Regex::new(r"^\-?(0|([1-9]\d*(\.\d+)?))$").unwrap();
    static ref DATE_PATTERN: Regex = Regex::new(r"^\d{4}\-\d{2}\-\d{2}$").unwrap();
}

#[derive(Debug, PartialEq)]
pub enum TokenType {
    String,
    Date,
    Number,
    Comment,
    OpenCurlyBrace,
    ClosingCurlyBrace,
    Plus,
    SemiColon,
    WhiteSpace,
    LineBreak,
    Other,
}

#[derive(Debug, PartialEq)]
pub struct Token<'a> {
    pub token_type: TokenType,
    pub span: (usize, usize),
    pub text: &'a str,
}

impl Token<'_> {
    pub fn is_whitespace(&self) -> bool {
        match self.token_type {
            TokenType::WhiteSpace => true,
            _ => false,
        }
    }

    pub fn is_line_break(&self) -> bool {
        match self.token_type {
            TokenType::LineBreak => true,
            _ => false,
        }
    }
}

pub trait DebugTokenExt {
    fn human_readable_string(&self) -> String;
}

impl DebugTokenExt for Token<'_> {
    /// Format the tokens into a nice, human readable string for troubleshooting purposes
    fn human_readable_string(&self) -> String {
        format!(
            "{:<20} {:<15} {:?}\n",
            format!("{:?}", self.token_type),
            format!("{} -> {}", self.span.0, self.span.1),
            self.text,
        )
    }
}

impl DebugTokenExt for Vec<Token<'_>> {
    /// Format the tokens into a nice, human readable string for troubleshooting purposes
    fn human_readable_string(&self) -> String {
        let mut output = String::new();

        for token in self {
            output.push_str(&token.human_readable_string());
        }

        output
    }
}

/// 1-based cursor position in a text file
pub struct TextPosition {
    line: usize,
    col: usize,
}

impl TextPosition {
    fn from_buffer_index(buffer: &[u8], index: usize) -> Self {
        let mut line = 1;
        let mut col = 1;

        for (i, c) in buffer.iter().enumerate() {
            if i == index {
                break;
            }

            if *c == NEWLINE {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        Self { line, col }
    }
}

impl core::fmt::Display for TextPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {} col {}", self.line, self.col)
    }
}

pub struct ScanIterator<'a> {
    buffer: &'a [u8],
    cursor: usize,
}

impl<'a> Iterator for ScanIterator<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match next_token(self.buffer, self.cursor).expect("Parse error") {
            Some((next_cursor, token)) => {
                self.cursor = next_cursor;
                Some(token)
            }
            None => None,
        }
    }
}

pub fn scan(buffer: &[u8]) -> ScanIterator {
    ScanIterator { buffer, cursor: 0 }
}

/// Reads the next token from the buffer, returns None on EOF
///
/// Also returns the position right after the last character in the token, so the caller can keep
/// calling this function until EOF.
///
/// Returns an error on lexer errors such as unterminated strings or comments.
///
fn next_token(buffer: &[u8], cursor: usize) -> Result<Option<(usize, Token)>, String> {
    let char = match buffer.get(cursor) {
        Some(char) => char,
        None => return Ok(None),
    };

    macro_rules! get_str {
        ($length:expr) => {
            str::from_utf8(buffer.get(cursor..cursor + $length).unwrap())
                .map_err(|err| format!("{}", err))?
        };
    }

    macro_rules! read_token {
        ($token_type:expr, $length:expr) => {{
            let token = Token {
                token_type: $token_type,
                span: (cursor, cursor + $length - 1),
                text: get_str!($length),
            };

            Ok(Some((cursor + $length, token)))
        }};
    }

    if *char == SEMICOLON {
        read_token!(TokenType::SemiColon, 1)
    } else if *char == PLUS {
        read_token!(TokenType::Plus, 1)
    } else if *char == LEFT_CURLY_BRACKET {
        read_token!(TokenType::OpenCurlyBrace, 1)
    } else if *char == RIGHT_CURLY_BRACKET {
        read_token!(TokenType::ClosingCurlyBrace, 1)
    } else if let Some(whitespace_length) = scan_whitespace(buffer, cursor) {
        read_token!(TokenType::WhiteSpace, whitespace_length)
    } else if let Some(line_break_length) = scan_line_break(buffer, cursor) {
        read_token!(TokenType::LineBreak, line_break_length)
    } else if let Some(string_length) = scan_string(buffer, cursor)? {
        read_token!(TokenType::String, string_length)
    } else if let Some(comment_length) = scan_comment(buffer, cursor) {
        read_token!(TokenType::Comment, comment_length)
    } else if let Some(comment_length) = scan_block_comment(buffer, cursor)? {
        read_token!(TokenType::Comment, comment_length)
    } else if let Some(token_length) = scan_other(buffer, cursor) {
        let str = get_str!(token_length);

        if NUMBER_PATTERN.is_match(str) {
            read_token!(TokenType::Number, token_length)
        } else if DATE_PATTERN.is_match(str) {
            read_token!(TokenType::Date, token_length)
        } else {
            read_token!(TokenType::Other, token_length)
        }
    } else {
        Err(format!(
            "Unexpected character at position {}: {:?}",
            cursor, *char as char,
        ))
    }
}

/// Checks if there is a string at the current position
///
/// Returns Ok(Some(string_length)) if there is a string at the current position, Ok(None) if
/// there isn't. Returns an error if the string is never terminated.
///
fn scan_string(buffer: &[u8], cursor: usize) -> Result<Option<usize>, String> {
    let quote_char = match buffer[cursor] {
        DOUBLE_QUOTE => DOUBLE_QUOTE,
        SINGLE_QUOTE => SINGLE_QUOTE,
        _ => return Ok(None), // This position doesn't start a string, exit early
    };

    let mut prev_char: Option<&u8> = None;

    let mut i = cursor + 1;

    loop {
        if let Some(char) = buffer.get(i) {
            let prev_char_is_backslash = match prev_char {
                Some(x) => *x == BACKSLASH,
                None => false,
            };

            // If the string is closed, we're done!
            if *char == quote_char && !prev_char_is_backslash {
                return Ok(Some(i + 1 - cursor));
            }

            prev_char = Some(char);
        } else {
            return Err(format!(
                "Unexpected end of input, string started at {} was never terminated",
                TextPosition::from_buffer_index(buffer, cursor),
            ));
        }

        i += 1;
    }
}

/// Checks if there is a single-line comment at the current position
fn scan_comment(buffer: &[u8], cursor: usize) -> Option<usize> {
    let is_forward_slash = |c: &u8| *c == SLASH;

    if !(buffer.get(cursor).map_or(false, is_forward_slash)
        && buffer.get(cursor + 1).map_or(false, is_forward_slash))
    {
        return None;
    }

    let mut length = 2;

    for i in cursor + 2.. {
        // Single-line comments last until the next line break or the end of the buffer
        if scan_line_break(buffer, i).is_some() || i == buffer.len() {
            break;
        }

        length += 1;
    }

    Some(length)
}

/// Checks if there is a block comment at the current position
fn scan_block_comment(buffer: &[u8], cursor: usize) -> Result<Option<usize>, String> {
    if !(buffer.get(cursor).map_or(false, |c| *c == SLASH)
        && buffer.get(cursor + 1).map_or(false, |c| *c == ASTERISK))
    {
        return Ok(None);
    }

    let mut length = 4;

    for i in cursor + 2.. {
        if i == buffer.len() {
            return Err(format!(
                "Unexpected end of input, block comment started at {} was never terminated",
                TextPosition::from_buffer_index(buffer, cursor)
            ));
        }

        if buffer.get(i).map_or(false, |c| *c == ASTERISK)
            && buffer.get(i + 1).map_or(false, |c| *c == SLASH)
        {
            break;
        }

        length += 1;
    }

    Ok(Some(length))
}

/// Checks if there is whitespace at the current position
fn scan_whitespace(buffer: &[u8], cursor: usize) -> Option<usize> {
    for i in cursor.. {
        if buffer
            .get(i)
            .map_or(false, |char| [SPACE, TAB].contains(char))
        {
            continue;
        } else {
            let len = i - cursor;

            return if len > 0 { Some(len) } else { None };
        }
    }

    None
}

/// Checks if there is a line break at this position
fn scan_line_break(buffer: &[u8], cursor: usize) -> Option<usize> {
    if buffer.get(cursor).map_or(false, |c| *c == b'\n') {
        Some(1)
    } else if buffer.get(cursor).map_or(false, |c| *c == b'\r')
        && buffer.get(cursor).map_or(false, |c| *c == b'\n')
    {
        Some(2)
    } else {
        None
    }
}

fn scan_other(buffer: &[u8], cursor: usize) -> Option<usize> {
    let mut i = cursor;

    while let Some(char) = buffer.get(i) {
        if is_delimiter(char) {
            break;
        }

        i += 1;
    }

    if i > cursor {
        Some(i - cursor)
    } else {
        None
    }
}

/// Reads until a non-whitespace character is found, returns the new cursor position
fn skip_whitespace(buffer: &[u8], cursor: usize) -> usize {
    let mut cursor = cursor;

    while let Some(char) = buffer.get(cursor) {
        if [SPACE, TAB, CARRIAGE_RETURN, NEWLINE].contains(char) {
            cursor += 1;
        } else {
            break;
        }
    }

    cursor
}

/// Returns true if this character should delimit a token
fn is_delimiter(c: &u8) -> bool {
    [
        SPACE,
        TAB,
        CARRIAGE_RETURN,
        NEWLINE,
        SEMICOLON,
        LEFT_CURLY_BRACKET,
        RIGHT_CURLY_BRACKET,
    ]
    .contains(c)
}

// /// Returns true if this is a valid YANG character
// ///
// /// See the definition of "yang-char" in the YANG ABNF grammar for more information.
// ///
// fn is_yang_char(c: &char) -> bool {
//     let ord = (*c) as u32;
//
//     return [0x09, 0x0A, 0x0D].contains(&ord)
//         || (0x20..=0xD7FF).contains(&ord)
//         || (0xE000..=0xFDCF).contains(&ord)
//         || (0xFDF0..=0xFFFD).contains(&ord)
//         || (0x10000..=0x1FFFD).contains(&ord)
//         || (0x20000..=0x2FFFD).contains(&ord)
//         || (0x30000..=0x3FFFD).contains(&ord)
//         || (0x40000..=0x4FFFD).contains(&ord)
//         || (0x50000..=0x5FFFD).contains(&ord)
//         || (0x60000..=0x6FFFD).contains(&ord)
//         || (0x70000..=0x7FFFD).contains(&ord)
//         || (0x80000..=0x8FFFD).contains(&ord)
//         || (0x90000..=0x9FFFD).contains(&ord)
//         || (0xA0000..=0xAFFFD).contains(&ord)
//         || (0xB0000..=0xBFFFD).contains(&ord)
//         || (0xC0000..=0xCFFFD).contains(&ord)
//         || (0xD0000..=0xDFFFD).contains(&ord)
//         || (0xE0000..=0xEFFFD).contains(&ord)
//         || (0xF0000..=0xFFFFD).contains(&ord)
//         || (0x100000..=0x10FFFD).contains(&ord);
// }

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn dedent(text: &str) -> String {
        let mut text = textwrap::dedent(text).trim().to_string();
        text.push('\n');
        text
    }

    #[test]
    fn smoke_test() {
        let buffer: Vec<u8> = dedent(
            r#"
             /*
              * This is a block comment
              */

             module test {
                 yang-version 1;
                 namespace "https://github.com/Hubro/yangparse";
                 description 'A small smoke test to make sure basic lexing works';

                 revision 2018-12-03 {
                     // I'm a comment!
                     description
                       "A multi-line string starting in an indented line

                        This is an idiomatic way to format large strings
                        in YANG models";
                 }

                 number 12.34;
             }
             "#,
        )
        .bytes()
        .collect();

        let tokens: Vec<_> = scan(&buffer).collect();

        assert_eq!(
            dedent(
                r#"
                Comment              0 -> 32         "/*\n * This is a block comment\n */"
                LineBreak            33 -> 33        "\n"
                LineBreak            34 -> 34        "\n"
                Other                35 -> 40        "module"
                WhiteSpace           41 -> 41        " "
                Other                42 -> 45        "test"
                WhiteSpace           46 -> 46        " "
                OpenCurlyBrace       47 -> 47        "{"
                LineBreak            48 -> 48        "\n"
                WhiteSpace           49 -> 52        "    "
                Other                53 -> 64        "yang-version"
                WhiteSpace           65 -> 65        " "
                Number               66 -> 66        "1"
                SemiColon            67 -> 67        ";"
                LineBreak            68 -> 68        "\n"
                WhiteSpace           69 -> 72        "    "
                Other                73 -> 81        "namespace"
                WhiteSpace           82 -> 82        " "
                String               83 -> 118       "\"https://github.com/Hubro/yangparse\""
                SemiColon            119 -> 119      ";"
                LineBreak            120 -> 120      "\n"
                WhiteSpace           121 -> 124      "    "
                Other                125 -> 135      "description"
                WhiteSpace           136 -> 136      " "
                String               137 -> 188      "'A small smoke test to make sure basic lexing works'"
                SemiColon            189 -> 189      ";"
                LineBreak            190 -> 190      "\n"
                LineBreak            191 -> 191      "\n"
                WhiteSpace           192 -> 195      "    "
                Other                196 -> 203      "revision"
                WhiteSpace           204 -> 204      " "
                Date                 205 -> 214      "2018-12-03"
                WhiteSpace           215 -> 215      " "
                OpenCurlyBrace       216 -> 216      "{"
                LineBreak            217 -> 217      "\n"
                WhiteSpace           218 -> 225      "        "
                Comment              226 -> 242      "// I'm a comment!"
                LineBreak            243 -> 243      "\n"
                WhiteSpace           244 -> 251      "        "
                Other                252 -> 262      "description"
                LineBreak            263 -> 263      "\n"
                WhiteSpace           264 -> 273      "          "
                String               274 -> 410      "\"A multi-line string starting in an indented line\n\n           This is an idiomatic way to format large strings\n           in YANG models\""
                SemiColon            411 -> 411      ";"
                LineBreak            412 -> 412      "\n"
                WhiteSpace           413 -> 416      "    "
                ClosingCurlyBrace    417 -> 417      "}"
                LineBreak            418 -> 418      "\n"
                LineBreak            419 -> 419      "\n"
                WhiteSpace           420 -> 423      "    "
                Other                424 -> 429      "number"
                WhiteSpace           430 -> 430      " "
                Number               431 -> 435      "12.34"
                SemiColon            436 -> 436      ";"
                LineBreak            437 -> 437      "\n"
                ClosingCurlyBrace    438 -> 438      "}"
                LineBreak            439 -> 439      "\n"
                "#
            ),
            tokens.human_readable_string(),
        );
    }

    #[test]
    fn test_string_concatenations() {
        let buffer: Vec<u8> = dedent(
            r#"
            type string {
                pattern '((:|[0-9a-fA-F]{0,4}):)([0-9a-fA-F]{0,4}:){0,5}'
                      + '((([0-9a-fA-F]{0,4}:)?(:|[0-9a-fA-F]{0,4}))|'
                      + '(((25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])\.){3}'
                      + '(25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])))'
                      + '(%[\p{N}\p{L}]+)?';
            }
            "#,
        )
        .bytes()
        .collect();

        let tokens: Vec<_> = scan(&buffer).collect();

        assert_eq!(
            dedent(
                r#"
                Other                0 -> 3          "type"
                WhiteSpace           4 -> 4          " "
                Other                5 -> 10         "string"
                WhiteSpace           11 -> 11        " "
                OpenCurlyBrace       12 -> 12        "{"
                LineBreak            13 -> 13        "\n"
                WhiteSpace           14 -> 17        "    "
                Other                18 -> 24        "pattern"
                WhiteSpace           25 -> 25        " "
                String               26 -> 74        "'((:|[0-9a-fA-F]{0,4}):)([0-9a-fA-F]{0,4}:){0,5}'"
                LineBreak            75 -> 75        "\n"
                WhiteSpace           76 -> 85        "          "
                Plus                 86 -> 86        "+"
                WhiteSpace           87 -> 87        " "
                String               88 -> 133       "'((([0-9a-fA-F]{0,4}:)?(:|[0-9a-fA-F]{0,4}))|'"
                LineBreak            134 -> 134      "\n"
                WhiteSpace           135 -> 144      "          "
                Plus                 145 -> 145      "+"
                WhiteSpace           146 -> 146      " "
                String               147 -> 194      "'(((25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])\\.){3}'"
                LineBreak            195 -> 195      "\n"
                WhiteSpace           196 -> 205      "          "
                Plus                 206 -> 206      "+"
                WhiteSpace           207 -> 207      " "
                String               208 -> 249      "'(25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])))'"
                LineBreak            250 -> 250      "\n"
                WhiteSpace           251 -> 260      "          "
                Plus                 261 -> 261      "+"
                WhiteSpace           262 -> 262      " "
                String               263 -> 281      "'(%[\\p{N}\\p{L}]+)?'"
                SemiColon            282 -> 282      ";"
                LineBreak            283 -> 283      "\n"
                ClosingCurlyBrace    284 -> 284      "}"
                LineBreak            285 -> 285      "\n"
                "#
            ),
            tokens.human_readable_string(),
        );
    }
}
