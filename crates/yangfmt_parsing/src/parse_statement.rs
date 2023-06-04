// Contains logic for parsing a YANG statement

use std::{iter::Peekable, slice::Iter};

use yangfmt_lexing::{Token, TokenType};

use crate::{NodeValue, ParseError, Statement};

#[derive(Debug)]
enum ParseState {
    Clean,

    /// State after having encountered a keyword
    ///
    ///     module foo {
    ///           ^
    ///
    GotKeyword(String, Vec<String>),

    /// State after having encountered a value
    ///
    ///     module foo {
    ///               ^
    ///
    GotValue(String, Vec<String>, NodeValue, Vec<String>),

    /// State after having encountered a plus symbol in a statement value
    ///
    ///     pattern "foo" + "bar";
    ///                    ^
    ///
    /// Statements with string concatenation values can't have value comments, since those comments
    /// will be associated with the last string instead.
    ///
    GotStringConcat(String, Vec<String>, Vec<(String, Vec<String>)>, PlusState),

    /// State after having found a keyword and a value, but before the line break
    ///
    ///     description "foo";
    ///                       ^
    ///     must "foo" {
    ///                 ^
    ///
    /// This is needed for finding any trailing comments that should be associated with the
    /// statement.
    ///
    GotStatement(
        String,
        Vec<String>,
        Option<NodeValue>,
        Vec<String>,
        bool,
        Vec<String>,
    ),
}

impl ParseState {
    fn new() -> Self {
        Self::Clean
    }

    fn got_keyword(keyword: String) -> Self {
        Self::GotKeyword(keyword, vec![])
    }

    fn got_value(keyword: String, keyword_comments: Vec<String>, value: NodeValue) -> Self {
        Self::GotValue(keyword, keyword_comments, value, vec![])
    }

    fn got_statement(
        keyword: String,
        keyword_comments: Vec<String>,
        value: Option<NodeValue>,
        value_comments: Vec<String>,
        has_children: bool,
    ) -> Self {
        Self::GotStatement(
            keyword,
            keyword_comments,
            value,
            value_comments,
            has_children,
            vec![],
        )
    }
}

/// Used in the string concatenation parse state to keep track of whether we're currently before or
/// after the plus symbol
#[derive(Debug)]
enum PlusState {
    BeforePlus,
    AfterPlus,
}

/// Tries to parse a YANG statement from a peekable iterator of Tokens
///
/// A statement includes everything up until and including the closing semicolon or opening curly
/// brace. Additionally, it contains any comments that are on the same line as the semicolon or
/// opening curly brace. Those comments are stored in "post_comments". This makes it easier to sort
/// statements without losing the comments associated with them.
///
/// This function doesn't recurse and parse statement children. Instead, the second value in the
/// returned tuple is a boolean that is set to "true" if the statement has children. In that case,
/// the parent function should parse every following statement as a child of this statement, until
/// a "}" is found.
///
/// NB: This function will consume and ignore any whitespace tokens after the statement, as it
/// searches for any comments to also consume as part of the statement.
///
pub fn parse_statement(
    token_stream: &mut Peekable<yangfmt_lexing::ScanIterator>,
) -> Result<(crate::Statement, bool), crate::ParseError> {
    let mut state = ParseState::new();
    let mut last_position: Option<usize> = None;

    // This loop parses the statement itself
    for token in token_stream.by_ref() {
        let token = match (token) {
            Ok(token) => token,
            Err(err) => return Err(err.into()),
        };

        last_position = Some(token.span.0);

        macro_rules! unexpected_token_error {
            () => {
                Err(ParseError {
                    message: format!("Unexpected token: {:?} ({:?})", token.text, token),
                    position: token.span.0,
                })
            };
        }
        match state {
            ParseState::Clean => match token.token_type {
                TokenType::Other => {
                    state = ParseState::got_keyword(token.text.into());
                }
                _ => {
                    return unexpected_token_error!();
                }
            },

            ParseState::GotKeyword(keyword, mut keyword_comments) => match token.token_type {
                // Ignore whitespace
                TokenType::WhiteSpace | TokenType::LineBreak => {
                    state = ParseState::GotKeyword(keyword, keyword_comments);
                }

                TokenType::Comment => {
                    keyword_comments.push(token.text.into());
                    state = ParseState::GotKeyword(keyword, keyword_comments);
                }

                TokenType::SemiColon => {
                    state =
                        ParseState::got_statement(keyword, keyword_comments, None, vec![], false);
                    break;
                }

                TokenType::OpenCurlyBrace => {
                    state =
                        ParseState::got_statement(keyword, keyword_comments, None, vec![], true);
                    break;
                }

                // Anything that isn't whitespace or a comment becomes the statement value
                _ => {
                    state = ParseState::got_value(keyword, keyword_comments, token.into());
                }
            },

            ParseState::GotValue(keyword, keyword_comments, value, mut value_comments) => {
                match token.token_type {
                    // Ignore whitespace
                    TokenType::WhiteSpace | TokenType::LineBreak => {
                        state =
                            ParseState::GotValue(keyword, keyword_comments, value, value_comments);
                    }

                    TokenType::Comment => {
                        value_comments.push(token.text.into());
                        state =
                            ParseState::GotValue(keyword, keyword_comments, value, value_comments);
                    }

                    TokenType::SemiColon => {
                        state = ParseState::got_statement(
                            keyword,
                            keyword_comments,
                            Some(value),
                            value_comments,
                            false,
                        );
                        break;
                    }

                    TokenType::OpenCurlyBrace => {
                        state = ParseState::got_statement(
                            keyword,
                            keyword_comments,
                            Some(value),
                            value_comments,
                            true,
                        );
                        break;
                    }

                    TokenType::Plus => match value {
                        NodeValue::String(value) => {
                            state = ParseState::GotStringConcat(
                                keyword,
                                keyword_comments,
                                vec![(value, value_comments)],
                                PlusState::AfterPlus,
                            );
                        }
                        _ => {
                            return Err(ParseError {
                                message: "Unexpected concatenation of non-string value".to_owned(),
                                position: token.span.0,
                            })
                        }
                    },

                    _ => {
                        return unexpected_token_error!();
                    }
                }
            }

            ParseState::GotStringConcat(keyword, keyword_comments, mut concat, plus_state) => {
                match plus_state {
                    PlusState::BeforePlus => match token.token_type {
                        TokenType::WhiteSpace | TokenType::LineBreak => {
                            state = ParseState::GotStringConcat(
                                keyword,
                                keyword_comments,
                                concat,
                                plus_state,
                            );
                        }
                        TokenType::Plus => {
                            state = ParseState::GotStringConcat(
                                keyword,
                                keyword_comments,
                                concat,
                                PlusState::AfterPlus,
                            );
                        }
                        TokenType::Comment => {
                            // Every comment encountered in the middle of a string concatenation is
                            // assumed to "belong" to the previous string
                            concat.last_mut().unwrap().1.push(token.text.to_owned());
                            state = ParseState::GotStringConcat(
                                keyword,
                                keyword_comments,
                                concat,
                                plus_state,
                            );
                        }
                        TokenType::SemiColon => {
                            state = ParseState::GotStatement(
                                keyword,
                                keyword_comments,
                                Some(NodeValue::StringConcatenation(concat)),
                                vec![],
                                false,
                                vec![],
                            );
                            break;
                        }
                        TokenType::OpenCurlyBrace => {
                            state = ParseState::GotStatement(
                                keyword,
                                keyword_comments,
                                Some(NodeValue::StringConcatenation(concat)),
                                vec![],
                                true,
                                vec![],
                            );
                            break;
                        }
                        _ => {
                            return unexpected_token_error!();
                        }
                    },
                    PlusState::AfterPlus => match token.token_type {
                        TokenType::WhiteSpace | TokenType::LineBreak => {
                            state = ParseState::GotStringConcat(
                                keyword,
                                keyword_comments,
                                concat,
                                plus_state,
                            );
                        }
                        TokenType::String => {
                            concat.push((token.text.to_owned(), vec![]));
                            state = ParseState::GotStringConcat(
                                keyword,
                                keyword_comments,
                                concat,
                                PlusState::BeforePlus,
                            );
                        }
                        TokenType::Comment => {
                            // Every comment encountered in the middle of a string concatenation is
                            // assumed to "belong" to the previous string
                            concat.last_mut().unwrap().1.push(token.text.to_owned());
                            state = ParseState::GotStringConcat(
                                keyword,
                                keyword_comments,
                                concat,
                                plus_state,
                            );
                        }
                        _ => return unexpected_token_error!(),
                    },
                }
            }

            ParseState::GotStatement(..) => {
                unreachable!("Loop should break when finishing a statement")
            }
        }
    }

    match state {
        ParseState::GotStatement(
            keyword,
            keyword_comments,
            value,
            value_comments,
            opens_block,
            mut post_comments,
        ) => {
            while let Some(token) = token_stream.peek() {
                let token = match (token) {
                    Ok(token) => token,
                    Err(err) => return Err(err.into()),
                };

                match token.token_type {
                    TokenType::WhiteSpace => {
                        token_stream.next();
                    }
                    TokenType::Comment => {
                        post_comments.push(token.text.to_string());
                        token_stream.next();
                    }
                    _ => break,
                }
            }

            Ok((
                Statement {
                    keyword: keyword.into(),
                    keyword_comments,
                    value,
                    value_comments,
                    children: None,
                    post_comments,
                },
                opens_block,
            ))
        }

        ParseState::Clean => unreachable!("Can't have a clean state after parsing a statement"),

        _ => Err(ParseError {
            message: "Unexpected end of input".to_string(),
            position: last_position.unwrap_or(0),
        }),
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::{NodeValue, Statement};

    use super::*;

    macro_rules! test_parse_statement {
        ($text:expr) => {{
            let bytes: Vec<u8> = $text.bytes().collect();
            let tokens = yangfmt_lexing::scan_iter(&bytes);
            let mut token_stream = tokens.peekable();
            parse_statement(&mut token_stream)
        }};
    }

    #[test]
    fn parse_keyword_only() {
        let (statement, opens_block) = test_parse_statement!("foo;").unwrap();

        assert_eq!(statement, Statement::new("foo"));
        assert_eq!(opens_block, false);
    }

    #[test]
    fn parse_keyword_and_value() {
        let (statement, opens_block) = test_parse_statement!("foo 123;").unwrap();

        assert_eq!(
            Statement::new("foo").with_value(NodeValue::Number("123".to_string())),
            statement,
        );
        assert_eq!(opens_block, false);

        let (statement, opens_block) = test_parse_statement!("foo \"bar\";").unwrap();

        assert_eq!(
            Statement::new("foo").with_value(NodeValue::String("\"bar\"".to_string())),
            statement,
        );
        assert_eq!(opens_block, false);

        let (statement, opens_block) = test_parse_statement!("foo bar;").unwrap();

        assert_eq!(
            Statement::new("foo").with_value(NodeValue::Other("bar".to_string())),
            statement,
        );
        assert_eq!(opens_block, false);
    }

    #[test]
    fn parse_string_concatenation() {
        let (statement, opens_block) = test_parse_statement!(r#"pattern "foo" + "bar";"#).unwrap();

        assert_eq!(
            Statement::new("pattern").with_value(NodeValue::StringConcatenation(vec![
                ("\"foo\"".to_string(), vec![],),
                ("\"bar\"".to_string(), vec![],),
            ],),),
            statement,
        );
        assert_eq!(opens_block, false);

        let (statement, opens_block) = test_parse_statement!(r#"pattern "foo" + "bar" {"#).unwrap();

        assert_eq!(
            Statement::new("pattern").with_value(NodeValue::StringConcatenation(vec![
                ("\"foo\"".to_string(), vec![],),
                ("\"bar\"".to_string(), vec![],),
            ],),),
            statement,
        );
        assert_eq!(opens_block, true);
    }

    #[test]
    fn parse_string_concatenation_comments() {
        let (statement, opens_block) = test_parse_statement!(
            r#"pattern "foo"  // Comment here
                  + "bar"// Another comments here
                  + "baz" /* several */ /* comments *//*here*/
                  ; // Semicolon on separate line because why not"#
        )
        .unwrap();

        assert_eq!(
            Statement::new("pattern")
                .with_value(NodeValue::StringConcatenation(vec![
                    ("\"foo\"".to_string(), vec!["// Comment here".to_string(),],),
                    (
                        "\"bar\"".to_string(),
                        vec!["// Another comments here".to_string(),],
                    ),
                    (
                        "\"baz\"".to_string(),
                        vec![
                            "/* several */".to_string(),
                            "/* comments */".to_string(),
                            "/*here*/".to_string(),
                        ],
                    ),
                ]))
                .with_post_comments(vec![
                    "// Semicolon on separate line because why not".to_string()
                ]),
            statement,
        );
        assert_eq!(opens_block, false);
    }

    #[test]
    fn parse_keyword_and_value_comments() {
        let (statement, opens_block) =
            test_parse_statement!("foo //bar  \n/* baz */123 // test\n  /*ouch*/ ;").unwrap();

        assert_eq!(
            Statement::new("foo")
                .with_keyword_comments(vec!["//bar  ".to_string(), "/* baz */".to_string()])
                .with_value(NodeValue::Number("123".to_string()))
                .with_value_comments(vec!["// test".to_string(), "/*ouch*/".to_string()]),
            statement
        );
        assert_eq!(opens_block, false);
    }

    #[test]
    fn opens_block() {
        let (statement, opens_block) = test_parse_statement!("foo {").unwrap();

        assert_eq!(Statement::new("foo"), statement);
        assert_eq!(true, opens_block);
    }

    #[test]
    fn post_comments() {
        let (statement, opens_block) =
            test_parse_statement!("foo; // post comment\n// not post comment").unwrap();

        assert_eq!(
            Statement::new("foo").with_post_comments(vec!["// post comment".to_string()]),
            statement
        );
        assert_eq!(false, opens_block);

        let (statement, opens_block) =
            test_parse_statement!("foo { // post comment\n// not post comment").unwrap();

        assert_eq!(
            Statement::new("foo").with_post_comments(vec!["// post comment".to_string()]),
            statement
        );
        assert_eq!(true, opens_block);
    }
}
