#![allow(unused)]

#[macro_use]
extern crate lazy_static;

mod constants;
mod node;
mod parse_statement;
mod parsing_dbg;

pub use crate::node::{Node, NodeValue, RootNode, Statement, StatementKeyword};
use crate::parse_statement::parse_statement;
use yangfmt_lexing::{LexerError, Token, TokenType};

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl<T> From<T> for ParseError
where
    T: std::borrow::Borrow<LexerError>,
{
    fn from(error: T) -> Self {
        let error = error.borrow();

        Self {
            message: error.message.clone(),
            position: error.position,
        }
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
pub fn parse(buffer: &[u8]) -> Result<RootNode, ParseError> {
    let mut tokens = yangfmt_lexing::scan_iter(buffer);
    let mut token_stream = tokens.peekable();

    let mut node_stack: Vec<Vec<Node>> = vec![vec![]];
    let mut prev_token_was_line_break = false;
    let mut prev_token_pos = 0;

    loop {
        let next_token = match token_stream.peek() {
            Some(Ok(token)) => token,
            Some(Err(error)) => return Err(error.into()),
            None => break,
        };

        let token_pos = next_token.span.0;
        let is_line_break = matches!(next_token.token_type, TokenType::LineBreak);
        let is_whitespace = matches!(next_token.token_type, TokenType::WhiteSpace);

        let mut nodes = node_stack.last_mut().expect("Stack should never be empty");

        match next_token.token_type {
            TokenType::WhiteSpace => {
                token_stream.next();
            }

            TokenType::LineBreak => {
                if prev_token_was_line_break {
                    nodes.push(Node::EmptyLine(next_token.text.into()))
                }

                token_stream.next();
            }

            TokenType::Comment => {
                nodes.push(Node::Comment(next_token.text.to_string()));
                token_stream.next();
            }

            TokenType::ClosingCurlyBrace => {
                let nodes = node_stack.pop().expect("Node stack can never be empty");

                let prev_nodes = match node_stack.last_mut() {
                    Some(nodes) => nodes,
                    None => {
                        return Err(ParseError {
                            message: "Unexpected closing curly brace".to_string(),
                            position: next_token.span.0,
                        })
                    }
                };

                match prev_nodes.last_mut() {
                    Some(Node::Statement(statement)) => statement.children = Some(nodes),
                    Some(_) | None => {
                        unreachable!("Previous node when closing a block must be a statement")
                    }
                }

                token_stream.next();
            }

            _ => {
                let (statement, opens_block) = parse_statement(&mut token_stream)?;

                nodes.push(Node::Statement(statement));

                if opens_block {
                    node_stack.push(vec![]);
                }
            }
        };

        if is_line_break {
            prev_token_was_line_break = true;
        } else if !is_whitespace {
            prev_token_was_line_break = false;
        }

        prev_token_pos = token_pos;
    }

    if node_stack.len() > 1 {
        return Err(ParseError {
            message: "Unclosed block at end of file".to_owned(),
            position: prev_token_pos,
        });
    }

    Ok(RootNode {
        children: node_stack
            .pop()
            .expect("Should be one node list in node stack after parsing is done"),
    })
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

            number 12.34; // Same-line comment
        }
        "#,
        // Expected output
        r#"
        (root
          (comment)
          [EmptyLine]
          (Keyword "module" Other
            (Keyword "yang-version" Number)
            (Keyword "namespace" String)
            (Keyword "description" String)
            [EmptyLine]
            (Keyword "revision" Date
              (comment)
              (Keyword "description" String))
            [EmptyLine]
            (ExtensionKeyword "ext:omg-no-value")
            [EmptyLine]
            (INVALID "number" Number <post-comment>)))
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
          (Keyword "module" <comment> <comment> Other <comment> <comment> <post-comment>)
          (comment))
        "#
    );
}
