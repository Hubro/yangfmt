use regex::Regex;

use crate::constants::STATEMENT_KEYWORDS;
use crate::lexing::{Token, TokenType};

lazy_static! {
    /// See "identifier" from ABNF
    static ref IDENTIFIER_PATTERN: Regex = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9\-_.]*$").unwrap();

    /// identifier ":" identifier - See "unknown-statement" from ABNF
    static ref EXT_KEYWORD_PATTERN: Regex =
        Regex::new(r"^[a-zA-Z_][a-zA-Z0-9\-_.]*:[a-zA-Z_][a-zA-Z0-9\-_.]*$").unwrap();
}

#[derive(Debug)]
pub enum StatementKeyword {
    Keyword(String),
    ExtensionKeyword(String),
    Invalid(String),
}

impl StatementKeyword {
    /// Shortcut for reading the keyword text
    pub fn text(&self) -> &str {
        match self {
            StatementKeyword::Keyword(text) => text,
            StatementKeyword::ExtensionKeyword(text) => text,
            StatementKeyword::Invalid(text) => text,
        }
    }
}

#[derive(Debug)]
pub enum Node {
    Statement(Statement),
    LineBreak(String),
    Comment(String),
}

pub trait NodeHelpers {
    fn is_line_break(&self) -> bool;
    fn is_comment(&self) -> bool;

    /// Retrieves a mutable reference to the node value, if any
    fn node_value_mut(&mut self) -> Option<&mut NodeValue>;

    /// Retrieves a mutable reference to the node value's text, if any
    fn value_string_mut(&mut self) -> Option<&mut String>;
}

impl NodeHelpers for Node {
    fn is_line_break(&self) -> bool {
        matches!(self, Node::LineBreak(_))
    }

    fn is_comment(&self) -> bool {
        matches!(self, Node::Comment(_))
    }

    fn node_value_mut(&mut self) -> Option<&mut NodeValue> {
        match self {
            Node::Statement(statement) => statement.value.as_mut(),
            _ => None,
        }
    }

    /// Retrieves a mutable reference to the node value text, if any
    fn value_string_mut(&mut self) -> Option<&mut String> {
        let value = match self {
            Node::Statement(statement) => &mut statement.value,
            _ => return None,
        };

        if let Some(ref mut value) = value {
            match value {
                NodeValue::String(ref mut text) => Some(text),
                NodeValue::Date(ref mut text) => Some(text),
                NodeValue::Number(ref mut text) => Some(text),
                NodeValue::Other(ref mut text) => Some(text),
                NodeValue::StringConcatenation(_) => None,
            }
        } else {
            None
        }
    }
}

impl NodeHelpers for Option<&Node> {
    fn is_line_break(&self) -> bool {
        self.map_or(false, |node| node.is_line_break())
    }
    fn is_comment(&self) -> bool {
        self.map_or(false, |node| node.is_comment())
    }
    fn node_value_mut(&mut self) -> Option<&mut NodeValue> {
        unimplemented!("Cannot implement on a non-mutable ref")
    }
    fn value_string_mut(&mut self) -> Option<&mut String> {
        unimplemented!("Cannot implement on a non-mutable ref")
    }
}

#[derive(Debug)]
pub struct RootNode {
    pub children: Vec<Node>,
}

#[derive(Debug)]
pub struct Statement {
    pub keyword: StatementKeyword,
    pub keyword_comments: Vec<String>, // Comment(s) between the statement keyword and value
    pub value: Option<NodeValue>,
    pub value_comments: Vec<String>, // Comment(s) between the value and block
    pub children: Option<Vec<Node>>,
}

/// The value part of a statement
#[derive(Debug)]
pub enum NodeValue {
    String(String),
    StringConcatenation(Vec<String>),
    Number(String),
    Date(String),

    /// Any value not obviously identifiable as a quoted string, number or date is just loosely
    /// categorized as "other". This can be extended to support more fine grained types such as
    /// identifiers, booleans, xpaths, keypaths and so on if a use-case appears.
    Other(String),
}

impl From<&Token<'_>> for StatementKeyword {
    fn from(token: &Token) -> Self {
        if STATEMENT_KEYWORDS.contains(&token.text) {
            StatementKeyword::Keyword(token.text.to_string())
        } else if EXT_KEYWORD_PATTERN.is_match(token.text) {
            StatementKeyword::ExtensionKeyword(token.text.to_string())
        } else {
            // Anything that is not a statement keyword or an extension keyword is invalid, but
            // we'll keep building the tree anyway.
            StatementKeyword::Invalid(token.text.to_string())
        }
    }
}

impl From<Token<'_>> for StatementKeyword {
    fn from(token: Token) -> Self {
        (&token).into()
    }
}

impl From<&Token<'_>> for NodeValue {
    fn from(token: &Token) -> Self {
        match token.token_type {
            TokenType::String => Self::String(token.text.to_string()),
            TokenType::Number => Self::Number(token.text.to_string()),
            TokenType::Date => Self::Date(token.text.to_string()),
            _ => Self::Other(token.text.to_string()),
        }
    }
}

impl From<Token<'_>> for NodeValue {
    fn from(token: Token) -> Self {
        (&token).into()
    }
}

/// Parses the input bytes as a YANG documents and returns a syntax tree
///
/// The returned node is a virtual "root" block node. This node contains the actual module or
/// sub-module node as one of its children, as well as any comments that are above or below that
/// node.
///
/// This parser doesn't strictly enforce the official grammar, and the returned tree may well be
/// invalid YANG. For example, this function will parse a document with multiple module blocks just
/// fine, or no module node at all, just a bunch of leafs.
///
pub fn parse(buffer: &[u8]) -> Result<RootNode, String> {
    let mut tokens = crate::lexing::scan(buffer);

    Ok(RootNode {
        children: parse_statements(&mut tokens)?,
    })
}

enum ParseState {
    Clean,
    GotKeyword(StatementKeyword, Vec<String>),
    GotValue(StatementKeyword, Vec<String>, NodeValue, Vec<String>),
    StringConcat(StatementKeyword, Vec<String>, bool),
}

fn parse_statements(tokens: &mut crate::lexing::ScanIterator) -> Result<Vec<Node>, String> {
    let mut statements: Vec<Node> = vec![];
    let mut state = ParseState::Clean;

    loop {
        match tokens.next() {
            Some(token) => {
                match state {
                    ParseState::Clean => {
                        // From a clean state, we expect to find a statement keyword, a comment or
                        // a closing curly brace
                        match token.token_type {
                            TokenType::WhiteSpace => {
                                // Ignore whitespace
                            }
                            TokenType::LineBreak => {
                                statements.push(Node::LineBreak(token.text.to_string()))
                            }
                            TokenType::Comment => {
                                statements.push(Node::Comment(token.text.to_string()))
                            }
                            TokenType::ClosingCurlyBrace => {
                                return Ok(statements);
                            }
                            TokenType::Other => {
                                state = ParseState::GotKeyword(token.into(), vec![])
                            }
                            _ => return Err(format!("Unexpected token: {:?}", token)),
                        }
                    }

                    ParseState::GotKeyword(keyword, mut keyword_comments) => {
                        match token.token_type {
                            TokenType::WhiteSpace => {
                                // Ignore whitespace
                                state = ParseState::GotKeyword(keyword, keyword_comments);
                            }

                            TokenType::LineBreak => {
                                statements.push(Node::LineBreak(token.text.to_string()));
                                state = ParseState::GotKeyword(keyword, keyword_comments);
                            }

                            TokenType::Comment => {
                                keyword_comments.push(token.text.to_string());
                                state = ParseState::GotKeyword(keyword, keyword_comments);
                            }

                            TokenType::OpenCurlyBrace => {
                                // Recurse!
                                statements.push(Node::Statement(Statement {
                                    keyword,
                                    keyword_comments,
                                    value: None,
                                    value_comments: vec![],
                                    children: Some(parse_statements(tokens)?),
                                }));

                                state = ParseState::Clean;
                            }

                            TokenType::SemiColon => {
                                statements.push(Node::Statement(Statement {
                                    keyword,
                                    keyword_comments,
                                    value: None,
                                    value_comments: vec![],
                                    children: None,
                                }));

                                state = ParseState::Clean;
                            }

                            _ => {
                                state = ParseState::GotValue(
                                    keyword,
                                    keyword_comments,
                                    token.into(),
                                    vec![],
                                );
                            }
                        }
                    }

                    ParseState::GotValue(keyword, keyword_comments, value, mut value_comments) => {
                        match token.token_type {
                            TokenType::WhiteSpace => {
                                // Ignore whitespace
                                state = ParseState::GotValue(
                                    keyword,
                                    keyword_comments,
                                    value,
                                    value_comments,
                                );
                            }
                            TokenType::LineBreak => {
                                statements.push(Node::LineBreak(token.text.to_string()));
                                state = ParseState::GotValue(
                                    keyword,
                                    keyword_comments,
                                    value,
                                    value_comments,
                                );
                            }

                            TokenType::Comment => {
                                value_comments.push(token.text.to_string());
                                state = ParseState::GotValue(
                                    keyword,
                                    keyword_comments,
                                    value,
                                    value_comments,
                                );
                            }

                            TokenType::OpenCurlyBrace => {
                                // Recurse!
                                statements.push(Node::Statement(Statement {
                                    keyword,
                                    keyword_comments,
                                    value: Some(value),
                                    value_comments,
                                    children: Some(parse_statements(tokens)?),
                                }));

                                state = ParseState::Clean;
                            }

                            TokenType::Plus => {
                                let value = match value {
                                    NodeValue::String(string) => string,
                                    _ => {
                                        return Err(format!(
                                            "Can only concatenate strings (pos {})",
                                            token.span.0
                                        ))
                                    }
                                };
                                state = ParseState::StringConcat(keyword, vec![value], true);
                            }

                            TokenType::SemiColon => {
                                statements.push(Node::Statement(Statement {
                                    keyword,
                                    keyword_comments,
                                    value: Some(value),
                                    value_comments,
                                    children: None,
                                }));

                                state = ParseState::Clean;
                            }

                            _ => {
                                return Err(format!(
                                    "Expected semicolon or block, got: {:?}",
                                    token
                                ));
                            }
                        }
                    }

                    ParseState::StringConcat(keyword, mut values, got_plus) => {
                        // Completely ignore whitespace and line breaks during a string
                        // concatenation
                        if token.is_whitespace() || token.is_line_break() {
                            state = ParseState::StringConcat(keyword, values, got_plus);
                            continue;
                        }

                        if got_plus {
                            // If the last symbol was a plus, the only valid token now is a string
                            match token.token_type {
                                TokenType::String => {
                                    values.push(token.text.to_string());
                                    state = ParseState::StringConcat(keyword, values, false);
                                }

                                _ => {
                                    return Err(format!(
                                        "Expected a string at position {}",
                                        token.span.0
                                    ))
                                }
                            }
                        } else {
                            // If we don't have a plus, the valid next tokens are a plus or a
                            // semicolon
                            match token.token_type {
                                TokenType::Plus => {
                                    state = ParseState::StringConcat(keyword, values, true);
                                }
                                TokenType::SemiColon => {
                                    statements.push(Node::Statement(Statement {
                                        keyword,
                                        keyword_comments: vec![],
                                        value: Some(NodeValue::StringConcatenation(values)),
                                        value_comments: vec![],
                                        children: None,
                                    }));
                                    state = ParseState::Clean;
                                }

                                _ => {
                                    return Err(format!(
                                        "Expected '+' or ';' at position {}",
                                        token.span.0
                                    ))
                                }
                            }
                        }
                    }
                }
            }

            // When we reach the end of the token stream, we're done and can return
            None => match state {
                ParseState::Clean => return Ok(statements),
                _ => return Err("Unexpected end of input".to_string()),
            },
        };
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn dedent(text: &str) -> String {
        let mut text = textwrap::dedent(text).trim().to_string();
        text.push('\n');
        text
    }

    macro_rules! test_parse {
        ($test_name:ident, $input:expr, $expected_output:expr) => {
            #[test]
            fn $test_name() {
                let buffer: Vec<u8> = dedent($input).bytes().collect();
                let tree = parse(&buffer).expect("Failed to parse YANG");

                assert_eq!(dedent($expected_output), tree.to_string());
            }
        };
    }

    test_parse!(
        smoke_test,
        // Input
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

            ext:omg-no-value;

            number 12.34;
        }
        "#,
        // Expected output
        r#"
        (root
          (comment)
          [LineBreak "\n"]
          [LineBreak "\n"]
          (Keyword "module" Other
            [LineBreak "\n"]
            (Keyword "yang-version" Number)
            [LineBreak "\n"]
            (Keyword "namespace" String)
            [LineBreak "\n"]
            (Keyword "description" String)
            [LineBreak "\n"]
            [LineBreak "\n"]
            (Keyword "revision" Date
              [LineBreak "\n"]
              (comment)
              [LineBreak "\n"]
              [LineBreak "\n"]
              (Keyword "description" String)
              [LineBreak "\n"])
            [LineBreak "\n"]
            [LineBreak "\n"]
            (ExtensionKeyword "ext:omg-no-value")
            [LineBreak "\n"]
            [LineBreak "\n"]
            (INVALID "number" Number)
            [LineBreak "\n"])
          [LineBreak "\n"])
        "#
    );

    test_parse!(
        really_try_to_break_shit_with_awful_comments,
        // Input
        r#"
        module   /*Why god, why*/  /* NO */ foo   // Why are you doing this
        /* Ewwwwww */{// WHY THOUGH
        }// Some of these comment locations have actually been spotted in the wild
        "#,
        // Expected output
        r#"
        (root
          [LineBreak "\n"]
          (Keyword "module" <comment> <comment> Other <comment> <comment>
            (comment)
            [LineBreak "\n"])
          (comment)
          [LineBreak "\n"])
        "#
    );
}
