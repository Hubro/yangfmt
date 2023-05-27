use regex::Regex;

use crate::constants::STATEMENT_KEYWORDS;
use yangfmt_lexing::{Token, TokenType};

lazy_static! {
    /// See "identifier" from ABNF
    static ref IDENTIFIER_PATTERN: Regex = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9\-_.]*$").unwrap();

    /// identifier ":" identifier - See "unknown-statement" from ABNF
    static ref EXT_KEYWORD_PATTERN: Regex =
        Regex::new(r"^[a-zA-Z_][a-zA-Z0-9\-_.]*:[a-zA-Z_][a-zA-Z0-9\-_.]*$").unwrap();
}

#[derive(Debug, PartialEq)]
pub enum Node {
    Statement(Statement),
    EmptyLine(String),
    Comment(String),
}

pub trait NodeHelpers {
    fn is_empty_line(&self) -> bool;
    fn is_comment(&self) -> bool;

    /// Retrieves a reference to the node value, if any
    fn node_value(&self) -> Option<&NodeValue>;

    /// Retrieves a mutable reference to the node value, if any
    fn node_value_mut(&mut self) -> Option<&mut NodeValue>;

    /// Retrieves a mutable reference to the node value's text, if any
    fn value_string_mut(&mut self) -> Option<&mut String>;
}

impl NodeHelpers for Node {
    fn is_empty_line(&self) -> bool {
        matches!(self, Node::EmptyLine(_))
    }

    fn is_comment(&self) -> bool {
        matches!(self, Node::Comment(_))
    }

    fn node_value(&self) -> Option<&NodeValue> {
        match self {
            Node::Statement(statement) => statement.value.as_ref(),
            _ => None,
        }
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
    fn is_empty_line(&self) -> bool {
        self.map_or(false, |node| node.is_empty_line())
    }
    fn is_comment(&self) -> bool {
        self.map_or(false, |node| node.is_comment())
    }
    fn node_value(&self) -> Option<&NodeValue> {
        match self {
            Some(Node::Statement(statement)) => statement.value.as_ref(),
            _ => None,
        }
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

#[derive(Debug, PartialEq)]
pub struct Statement {
    pub keyword: StatementKeyword,
    /// Comment(s) between the statement keyword and value
    pub keyword_comments: Vec<String>,
    pub value: Option<NodeValue>,
    /// Comment(s) between the value and block
    pub value_comments: Vec<String>,
    pub children: Option<Vec<Node>>,
    /// Any comments after the statement, but on the same line. For single-line statements, this is
    /// any comments after the semicolon. For block statements, this is any comment after the
    /// opening brace, on the same line.
    pub post_comments: Vec<String>,
}

impl Statement {
    pub fn new(keyword: impl AsRef<str>) -> Self {
        Self {
            keyword: keyword.as_ref().into(),
            keyword_comments: vec![],
            value: None,
            value_comments: vec![],
            children: None,
            post_comments: vec![],
        }
    }

    pub fn with_keyword_comments(self, keyword_comments: Vec<String>) -> Self {
        Self {
            keyword_comments,
            ..self
        }
    }

    pub fn with_value(self, value: NodeValue) -> Self {
        Self {
            value: Some(value),
            ..self
        }
    }

    pub fn with_value_comments(self, value_comments: Vec<String>) -> Self {
        Self {
            value_comments,
            ..self
        }
    }

    pub fn with_post_comments(self, post_comments: Vec<String>) -> Self {
        Self {
            post_comments,
            ..self
        }
    }
}

#[derive(Debug, PartialEq)]
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

impl From<&str> for StatementKeyword {
    fn from(value: &str) -> Self {
        if STATEMENT_KEYWORDS.contains(&value) {
            StatementKeyword::Keyword(value.into())
        } else if EXT_KEYWORD_PATTERN.is_match(value) {
            StatementKeyword::ExtensionKeyword(value.into())
        } else {
            // Anything that is not a statement keyword or an extension keyword is invalid, but
            // we'll keep building the tree anyway.
            StatementKeyword::Invalid(value.into())
        }
    }
}

impl From<String> for StatementKeyword {
    fn from(value: String) -> Self {
        if STATEMENT_KEYWORDS.contains(&value.as_str()) {
            StatementKeyword::Keyword(value)
        } else if EXT_KEYWORD_PATTERN.is_match(&value) {
            StatementKeyword::ExtensionKeyword(value)
        } else {
            // Anything that is not a statement keyword or an extension keyword is invalid, but
            // we'll keep building the tree anyway.
            StatementKeyword::Invalid(value)
        }
    }
}

impl From<&Token<'_>> for StatementKeyword {
    fn from(token: &Token) -> Self {
        token.text.into()
    }
}

impl From<Token<'_>> for StatementKeyword {
    fn from(token: Token) -> Self {
        token.text.into()
    }
}

/// The value part of a statement
#[derive(Debug, PartialEq)]
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
