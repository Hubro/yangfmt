use crate::parsing::{parse, Node, NodeHelpers, NodeValue, Statement, StatementKeyword};

pub enum Indent {
    // Tab,
    Spaces(u8),
}

pub struct FormatConfig {
    pub indent: Indent,
    pub line_length: u16,
}

impl FormatConfig {
    fn indent_width(&self) -> u8 {
        match self.indent {
            // Indent::Tab
            Indent::Spaces(num) => num,
        }
    }
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
    let mut tree = parse(buffer)?;

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
        if let Node::Statement(ref mut statement) = node {
            add_block_line_breaks(statement);

            // Recurse into the block node's children
            if let Some(ref mut children) = statement.children {
                process_statements(children);
            }
        }

        convert_to_double_quotes(node);
    }

    trim_line_breaks(statements);
    squash_line_breaks(statements);
    relocate_pre_block_comments(statements);
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
fn add_block_line_breaks(stmt: &mut Statement) {
    if let Some(ref mut children) = stmt.children {
        if !children.get(0).map_or(false, |child| child.is_line_break()) {
            children.insert(0, Node::LineBreak(String::from("\n")));
        }

        if !children.last().map_or(false, |child| child.is_line_break()) {
            children.push(Node::LineBreak(String::from("\n")));
        }
    }
}

/// Relocates keyword- and value comments somewhere more acceptable
///
/// See tests at the bottom of the file for example results.
///
fn relocate_pre_block_comments(nodes: &mut [Node]) {
    for node in nodes.iter_mut() {
        if let Node::Statement(stmt) = node {
            // Only move keyword-comments or value-comments if this statement has a block
            if stmt.children.is_none() {
                continue;
            }

            if stmt.value.is_some() {
                // If the statement has a value, we want to move every value comment into the
                // children
                while let Some(comment) = stmt.value_comments.pop() {
                    if let Some(ref mut children) = stmt.children {
                        // If this is a block, move the value comments into the block children
                        children.insert(0, Node::Comment(comment))
                    }
                }
            } else {
                // If the statement doesn't have a value, we instead want to move every keyword
                // comment into the children
                while let Some(comment) = stmt.keyword_comments.pop() {
                    if let Some(ref mut children) = stmt.children {
                        // If this is a block, move the value comments into the block children
                        children.insert(0, Node::Comment(comment))
                    }
                }
            }
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

    if statements.last().is_line_break() && statements.len() > 1 {
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

/// Converts single-quoted strings to double quoted strings
///
/// The only exception is if the string contains double-quotes.
///
fn convert_to_double_quotes(node: &mut Node) {
    let is_single_quoted = |str: &str| str.bytes().next().map_or(false, |byte| byte == b'\'');

    let contains_quote = |str: &str| {
        let mut content = str.chars();
        content.next();
        content.next_back();

        let content = content.as_str();

        content.contains('\"')
    };

    let set_double_quotes = |str: &mut String| {
        str.replace_range(0..1, "\"");
        str.replace_range(str.len() - 1.., "\"");
    };

    if let Some(NodeValue::String(string)) = node.node_value_mut() {
        if !is_single_quoted(string) || contains_quote(string) {
            return;
        }

        set_double_quotes(string);
    }

    if let Some(NodeValue::StringConcatenation(strings)) = node.node_value_mut() {
        for string in strings {
            if !is_single_quoted(string) || contains_quote(string) {
                continue;
            }

            set_double_quotes(string);
        }
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
                    // Indent::Tab => {
                    //     write!(out, "\t")?;
                    // }
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
        ($node:expr) => {
            match $node.keyword {
                StatementKeyword::Keyword(ref text) => write!(out, "{text}")?,
                StatementKeyword::ExtensionKeyword(ref text) => write!(out, "{text}")?,
                StatementKeyword::Invalid(ref text) => write!(out, "{text}")?,
            };

            for comment in $node.keyword_comments.as_slice() {
                write!(out, " {comment}")?;
            }

            // This is where keyword comment would be written, but since the formatting rules will
            // move them all, there will never be anything to write.
        };
    }

    macro_rules! write_simple_value {
        ($line_pos:expr, $value:expr) => {{
            // Line length = indent + keyword + value + a space + a semicolon
            if ($line_pos + ($value.len() as u16) + 2 > config.line_length) {
                writeln!(out)?;
                indent!(depth + 1);
            } else {
                write!(out, " ")?;
            }

            write!(out, "{}", $value)?;
        }};
    }

    macro_rules! write_value {
        ($node:expr) => {
            let kw_text = $node.keyword.text();
            let line_pos: u16 = (config.indent_width() as u16) * depth + (kw_text.len() as u16);

            match $node.value.as_ref().unwrap() {
                NodeValue::Date(text) => write_simple_value!(line_pos, text),
                NodeValue::Number(text) => write_simple_value!(line_pos, text),
                NodeValue::String(text) => write_simple_value!(line_pos, text),
                NodeValue::Other(text) => write_simple_value!(line_pos, text),
                NodeValue::StringConcatenation(strings) => {
                    let kwlen = kw_text.len();
                    let pad = if kwlen >= 2 { kwlen - 2 } else { 0 };

                    // The first string gets written on the same line as the keywords
                    write!(out, " {}", strings[0])?;

                    // The rest get displayed on new lines, padded to align with the first string
                    if let Some(rest) = strings.get(1..) {
                        for ref string in rest {
                            writeln!(out)?;
                            indent!(depth);

                            for _ in 0..pad {
                                write!(out, " ")?
                            }

                            write!(out, " + {}", string)?;
                        }
                    }
                }
            };

            for comment in $node.value_comments.as_slice() {
                write!(out, " {comment}")?;
            }
        };
    }

    match node {
        Node::Statement(node) => {
            write_keyword!(node);

            if node.value.is_some() {
                write_value!(node);
            }

            if let Some(ref children) = node.children {
                write!(out, " {{")?;

                // It's often useful to know what the previous child node was
                let mut prev_child: Option<&Node> = None;

                for child in children.as_slice() {
                    if !child.is_line_break() {
                        // If the previous line was a line break, draw indentation now, except if the
                        // current node is also a line break. We don't want indentation on empty lines.
                        if prev_child.is_line_break() {
                            indent!(depth + 1);
                        }

                        // If there is no line break after the "{" then add a space before the next
                        // token
                        if prev_child.is_none() {
                            write!(out, " ")?;
                        }

                        // If the previous node was not a line break, add a space before writing this
                        // node
                        if prev_child.is_some() && !prev_child.is_line_break() {
                            write!(out, " ")?;
                        }
                    }

                    write_node(out, child, config, depth + 1)?;

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
            } else {
                write!(out, ";")?;
            }
        }

        Node::Comment(text) => {
            write!(out, "{text}")?;
        }

        Node::LineBreak(_) => {
            writeln!(out)?;
        }
    }

    Ok(())
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

    /// Formats the input file into a String
    fn format_yang_str(buffer: &[u8], config: &FormatConfig) -> Result<String, Error> {
        let mut output: Vec<u8> = vec![];

        format_yang(&mut output, buffer, config)?;

        Ok(String::from_utf8(output).expect("Invalid UTF-8 in input file"))
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

        write_node(&mut out, module_node, &config, 0).expect("Formatting failed");
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

                bar      testing  ;
                foo      123.45   ;

                revision 2022-02-03 {
                }
                    revision 2022-02-02
                    { description "qwerty"; }

                //
                // Some string formatting tests
                //

                test "I am not affected";
                test 'I am converted';
                test 'These "quotes" should remain single';

                description "I am short and sweet";
                description "I should stay on this line line <----------------->";
                description "I should be wrapped to the next line <------------->";
                description "I am multi-lined,
                    so I automatically get wrapped
                    to the next line even though each
                    individual line is short.";

                pattern '((:|[0-9a-fA-F]{0,4}):)([0-9a-fA-F]{0,4}:){0,5}'+'((([0-9a-fA-F]{0,4}:)?(:|[0-9a-fA-F]{0,4}))|'
                + '(((25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])\.){3}'
                 + '(25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])))'
                + '(%[\p{N}\p{L}]+)?';

                pattern
                "foo" + 'bar'
                + 'baz';

                augment "/foo"+"/bar"
                +"/baz"
                {

                }

                //
                // Empty blocks
                //

                test{}

                test{
                }

                test{

                }

                //
                // Comments
                //

                test // This sometimes happens and must be supported
                {
                    foo bar;
                }

                test "something" // This sometimes happens and must be supported
                {
                    foo bar;
                }

                test "foo" /* This would be weird */ /* But let's support it anyway */
                {
                    foo bar;
                }

                test /* foo */ /* bar */ /* baz */ "foo" /* pow */
                {
                    // Nobody's ever going to do this (hopefully) so let's not even bother trying
                    // to make it prettier. Just don't crash.
                }

                test "foo"; // A comment here is fine
                test "foo" /* This however, is not fine*/ ;
                test /* Nobody would ever do this, let's just not crash */ "foo" /* yuck */ ;
                }"#,
            )
            .as_bytes(),
            &(FormatConfig {
                indent: Indent::Spaces(4),
                line_length: 70,
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
                    bar testing;
                    foo 123.45;

                    revision 2022-02-03 {
                    }
                    revision 2022-02-02 {
                        description "qwerty";
                    }

                    //
                    // Some string formatting tests
                    //

                    test "I am not affected";
                    test "I am converted";
                    test 'These "quotes" should remain single';

                    description "I am short and sweet";
                    description "I should stay on this line line <----------------->";
                    description
                        "I should be wrapped to the next line <------------->";
                    description
                        "I am multi-lined,
                    so I automatically get wrapped
                    to the next line even though each
                    individual line is short.";

                    pattern "((:|[0-9a-fA-F]{0,4}):)([0-9a-fA-F]{0,4}:){0,5}"
                          + "((([0-9a-fA-F]{0,4}:)?(:|[0-9a-fA-F]{0,4}))|"
                          + "(((25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])\.){3}"
                          + "(25[0-5]|2[0-4][0-9]|[01]?[0-9]?[0-9])))"
                          + "(%[\p{N}\p{L}]+)?";

                    pattern "foo"
                          + "bar"
                          + "baz";

                    augment "/foo"
                          + "/bar"
                          + "/baz" {
                    }

                    //
                    // Empty blocks
                    //

                    test {
                    }

                    test {
                    }

                    test {
                    }

                    //
                    // Comments
                    //

                    test { // This sometimes happens and must be supported
                        foo bar;
                    }

                    test "something" { // This sometimes happens and must be supported
                        foo bar;
                    }

                    test "foo" { /* This would be weird */ /* But let's support it anyway */
                        foo bar;
                    }

                    test /* foo */ /* bar */ /* baz */ "foo" { /* pow */
                        // Nobody's ever going to do this (hopefully) so let's not even bother trying
                        // to make it prettier. Just don't crash.
                    }

                    test "foo"; // A comment here is fine
                    test "foo" /* This however, is not fine*/;
                    test /* Nobody would ever do this, let's just not crash */ "foo" /* yuck */;
                }
                "#
            ),
            result,
        );
    }
}
