#![allow(dead_code, unused_imports, unused_variables, unused_macros)]

use crate::lexing::TokenType;
use crate::parsing::{
    parse, BlockNode, LeafNode, Node, NodeHelpers, NodeValue, RootNode, StatementKeyword,
};

use std::io::{stdin, stdout, Read};

pub enum Indent {
    Tab,
    Spaces(u8),
}

pub struct FormatConfig {
    pub indent: Indent,
    pub line_length: u16,
}

#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Error {
    fn from(error: String) -> Self {
        Self(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self(format!("I/O Error: {}", error))
    }
}

/// Formats an input buffer of YANG source into the given output
pub fn format_yang<T: std::io::Write>(
    out: &mut T,
    buffer: &[u8],
    config: &FormatConfig,
) -> Result<(), Error> {
    let mut tree = parse(&buffer)?;

    process_statements(&mut tree.children);

    // The file should end with a line break
    if !tree.children.last().is_line_break() {
        tree.children.push(Node::LineBreak("\n".to_string()));
    }

    for node in tree.children {
        write_node(out, &node, config, 0)?;
    }

    Ok(())
}

/// Applies auto-formatting rules recursively to the input statement list
fn process_statements(statements: &mut Vec<Node>) {
    for ref mut node in statements.as_mut_slice() {
        if let Node::BlockNode(ref mut block_node) = node {
            add_block_line_breaks(block_node);

            // Recurse into the block node's children
            process_statements(&mut block_node.children);
        }
    }

    trim_line_breaks(statements);
    squash_line_breaks(statements);
}

/// Adds line breaks at the start of- and after every block node
///
/// Essentially converts every:
///
///     revision 2022-12-31 { ... }
///
/// Into:
///
///     revition 2022-12-31 {
///         ...
///     }
///
fn add_block_line_breaks(node: &mut BlockNode) {
    if node.children.len() > 0 {
        if !node.children[0].is_line_break() {
            node.children.insert(0, Node::LineBreak(String::from("\n")));
        }

        if !node.children.last().unwrap().is_line_break() {
            node.children.push(Node::LineBreak(String::from("\n")));
        }
    }
}

/// Removes leading and trailing line breaks from the statement list
///
/// Essentially converts:
///
///     foo {
///
///         bar {
///
///             description "Test";
///
///             reference "Test";
///
///
///         }
///
///     }
///
/// Into:
///
///     foo {
///         bar {
///             description "Test";
///
///             reference "Test";
///         }
///     }
///
fn trim_line_breaks(statements: &mut Vec<Node>) {
    if statements.get(0).is_line_break() {
        while statements.get(1).is_line_break() {
            statements.remove(1);
        }
    }

    if statements.last().is_line_break() {
        while statements.get(statements.len() - 2).is_line_break() {
            statements.remove(statements.len() - 2);
        }
    }
}

/// Squashes any occurrance of 3 or more line breaks down to 2 line breaks
///
/// Essentially converts:
///
///     module foo {
///         foo "123";
///
///
///
///         bar "123";
///     }
///
/// Into:
///
///     module foo {
///         foo "123";
///
///         bar "123";
///     }
///
fn squash_line_breaks(statements: &mut Vec<Node>) {
    // Start at second index, since this is the earliest possible position we'd want to prune any
    // line breaks
    let mut i = 2;

    while let Some(node) = statements.get(i) {
        if node.is_line_break()
            && statements.get(i - 1).is_line_break()
            && statements.get(i - 2).is_line_break()
        {
            statements.remove(i);
            continue;
        }

        i += 1;
    }
}

/// Writes the node tree to the given writeable object
///
/// This automatically handles indentation and spacing between nodes. However, it does not process
/// node order, line breaks and things like that. That is handled by a pre-processing step.
///
/// (This function leaves no trailing line break)
///
fn write_node<T: std::io::Write>(
    out: &mut T,
    node: &Node,
    config: &FormatConfig,
    depth: u16,
) -> Result<(), Error> {
    macro_rules! indent {
        ($depth:expr) => {
            for _ in 0..$depth {
                match config.indent {
                    Indent::Tab => {
                        write!(out, "\t")?;
                    }
                    Indent::Spaces(spaces) => {
                        for _ in 0..spaces {
                            write!(out, " ")?;
                        }
                    }
                }
            }
        };
    }

    macro_rules! write_keyword {
        ($keyword:expr) => {
            match $keyword {
                StatementKeyword::Keyword(text) => write!(out, "{text}")?,
                StatementKeyword::ExtensionKeyword(text) => write!(out, "{text}")?,
                StatementKeyword::Invalid(text) => write!(out, "{text}")?,
            };
        };
    }

    macro_rules! write_value {
        ($keyword:expr, $value:expr) => {
            match $value {
                NodeValue::Date(text) => write!(out, "{text}")?,
                NodeValue::Number(text) => write!(out, "{text}")?,
                NodeValue::String(text) => write!(out, "{text}")?,
                NodeValue::Other(text) => write!(out, "{text}")?,
                NodeValue::StringConcatenation(strings) => {
                    let kwlen = $keyword.text().len();
                    let pad = if kwlen >= 2 { kwlen - 2 } else { 0 };

                    write!(out, "{}", strings[0])?;

                    for ref string in strings {
                        writeln!(out)?;
                        indent!(depth);

                        for _ in 0..pad {
                            write!(out, " ")?
                        }

                        write!(out, " + {}", string)?;
                    }
                }
            };
        };
    }

    match node {
        Node::LeafNode(node) => {
            write_keyword!(&node.keyword);
            write!(out, " ")?;

            if let Some(ref value) = node.value {
                write_value!(&node.keyword, value);
            }

            write!(out, ";")?;
        }

        Node::BlockNode(node) => {
            write_keyword!(&node.keyword);
            write!(out, " ")?;

            if let Some(ref value) = node.value {
                write_value!(&node.keyword, value);
                write!(out, " ")?;
            }

            write!(out, "{{")?;

            // It's often useful to know what the previous child node was
            let mut prev_child: Option<&Node> = None;

            for child in node.children.as_slice() {
                if !child.is_line_break() {
                    // If the previous line was a line break, draw indentation now, except if the
                    // current node is also a line break. We don't want indentation on empty lines.
                    if prev_child.is_line_break() {
                        indent!(depth + 1);
                    }

                    // If there is no line break after the "{" then add a space before the next
                    // token
                    if !prev_child.is_some() {
                        write!(out, " ")?;
                    }

                    // If the previous node was not a line break, add a space before writing this
                    // node
                    if prev_child.is_some() && !prev_child.is_line_break() {
                        write!(out, " ")?;
                    }
                }

                write_node(out, &child, config, depth + 1)?;

                prev_child = Some(child);
            }

            if prev_child.is_line_break() {
                // If there is a line break before the closing "}", indent it
                indent!(depth);
            } else {
                // Otherwise, add a space before it
                write!(out, " ")?;
            }

            write!(out, "}}")?;
        }

        Node::CommentNode(text) => {
            write!(out, "{text}")?;
        }

        Node::LineBreak(_) => {
            writeln!(out)?;
        }
    }

    Ok(())
}

/// Formats the input file into a String
fn format_yang_str(buffer: &[u8], config: &FormatConfig) -> Result<String, Error> {
    let mut output: Vec<u8> = vec![];

    format_yang(&mut output, buffer, config)?;

    Ok(String::from_utf8(output).expect("Invalid UTF-8 in input file"))
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io::Write;

    fn dedent(text: &str) -> String {
        let mut text = textwrap::dedent(text).trim().to_string();
        text.push('\n');
        text
    }

    #[test]
    fn test_write_node() {
        let input_string = dedent(
            r#"
                module foo {
                bar "testing" ;
                foo 123.45    ;


                        revision 2022-02-02 {description "qwerty";} oh "dear";

                }
                "#,
        );

        let tree = parse(input_string.as_bytes()).expect("Failed to parse input");
        let module_node = tree.children.get(0).expect("Failed to get module node");

        let mut out: Vec<u8> = vec![];

        let config = FormatConfig {
            indent: Indent::Spaces(4),
            line_length: 80,
        };

        write_node(&mut out, &module_node, &config, 0).expect("Formatting failed");
        writeln!(out).unwrap();

        assert_eq!(
            dedent(
                r#"
                module foo {
                    bar "testing";
                    foo 123.45;


                    revision 2022-02-02 { description "qwerty"; } oh "dear";

                }
                "#
            ),
            String::from_utf8(out).unwrap(),
        );
    }

    #[test]
    fn test_format() {
        let result = format_yang_str(
            dedent(
                r#"
                //
                // Comments outside the module block should be fine
                //
                module foo {

                bar "testing" ;
                foo 123.45    ;

                    revision 2022-02-02 { description "qwerty"; }



                pattern '((:|[0-9a-fA-F]{0,4}):)([0-9a-fA-F]{0,4}:){0,5}'+'((([0-9a-fA-F]{0,4}:)?(:|[0-9a-fA-F]{0,4}))|'
                + '(((25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])\.){3}'
                 + '(25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])))'
                + '(%[\p{N}\p{L}]+)?';

                }"#,
            )
            .as_bytes(),
            &(FormatConfig {
                indent: Indent::Spaces(4),
                line_length: 80,
            }),
        )
        .unwrap();

        assert_eq!(
            dedent(
                r#"
                //
                // Comments outside the module block should be fine
                //
                module foo {
                    bar "testing";
                    foo 123.45;

                    revision 2022-02-02 {
                        description "qwerty";
                    }

                    pattern '((:|[0-9a-fA-F]{0,4}):)([0-9a-fA-F]{0,4}:){0,5}'
                          + '((:|[0-9a-fA-F]{0,4}):)([0-9a-fA-F]{0,4}:){0,5}'
                          + '((([0-9a-fA-F]{0,4}:)?(:|[0-9a-fA-F]{0,4}))|'
                          + '(((25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])\.){3}'
                          + '(25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])))'
                          + '(%[\p{N}\p{L}]+)?';
                }
                "#
            ),
            result,
        );
    }
}