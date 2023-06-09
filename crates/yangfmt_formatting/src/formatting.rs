mod canonical_order;

use yangfmt_parsing::{parse, Node, NodeHelpers, NodeValue, ParseError, StatementKeyword};

use crate::canonical_order::sort_statements;

pub enum Indent {
    // Tab,
    Spaces(u8),
}

pub struct FormatConfig {
    pub indent: Indent,
    pub line_length: u16,
    pub fix_canonical_order: bool,
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
pub enum Error {
    ParseError(ParseError),
    IOError(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ParseError(parse_error) => write!(f, "{}", parse_error.message),
            Error::IOError(text) => write!(f, "{}", text),
        }
    }
}

impl From<ParseError> for Error {
    fn from(value: ParseError) -> Self {
        Self::ParseError(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::IOError(format!("I/O Error: {}", error))
    }
}

/// Formats an input buffer of YANG source into the given output
pub fn format_yang<T: std::io::Write>(
    out: &mut T,
    buffer: &[u8],
    config: &FormatConfig,
) -> Result<(), Error> {
    let mut tree = parse(buffer)?;

    process_statements(None, &mut tree.children, config);

    for node in tree.children {
        write_node(out, &node, config, 0)?;
    }

    Ok(())
}

/// Applies auto-formatting rules recursively to the input statement list
fn process_statements(
    parent_node_name: Option<&str>,
    statements: &mut Vec<Node>,
    config: &FormatConfig,
) {
    for node in statements.as_mut_slice() {
        if let Node::Statement(ref mut statement) = node {
            // Recurse into the block node's children
            if let Some(ref mut children) = statement.children {
                process_statements(Some(statement.keyword.text()), children, config);
            }
        }

        convert_to_double_quotes(node);
        strip_string(node);

        // Multi-lined quoted strings get stripped and dedented
        dedent_multilined_string(node);
    }

    trim_line_breaks(statements);
    squash_line_breaks(statements);
    relocate_pre_block_comments(statements);

    if config.fix_canonical_order {
        sort_statements(parent_node_name, statements);
    }
}

/// Relocates keyword- and value comments somewhere more acceptable
///
/// See tests at the bottom of the file for example results.
///
fn relocate_pre_block_comments(nodes: &mut [Node]) {
    for node in nodes.iter_mut() {
        if let Node::Statement(stmt) = node {
            // Move all keyword comments and value comments into the post comments
            stmt.post_comments.append(&mut stmt.keyword_comments);
            stmt.post_comments.append(&mut stmt.value_comments);
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
    while statements.get(0).is_empty_line() {
        statements.remove(0);
    }

    while statements.last().is_empty_line() {
        statements.pop();
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
    let mut i = 1;

    while let Some(node) = statements.get(i) {
        if node.is_empty_line() && statements.get(i - 1).is_empty_line() {
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
        for (ref mut string, _) in strings {
            if !is_single_quoted(string) || contains_quote(string) {
                continue;
            }

            set_double_quotes(string);
        }
    }
}

/// Strips all leading and trailing whitespace from string values
fn strip_string(node: &mut Node) {
    if let Some(NodeValue::String(ref mut text)) = node.node_value_mut() {
        let slice = text.as_str();
        let slice = &slice[1..slice.len() - 1]; // Without the quotes

        let text_start = 1 + match slice.find(|c: char| !c.is_ascii_whitespace()) {
            Some(pos) => pos,
            None => {
                // None means the string doesn't contain any non-whitespace characters, just
                // replace it with an empty string
                text.clear();
                text.push_str("\"\"");
                return;
            }
        };

        let text_end = text.len()
            - slice
                .chars()
                .rev()
                .position(|c| !c.is_whitespace())
                .unwrap_or(0)
            - 2;

        if text_end < (text.len() - 2) {
            text.drain(text_end + 1..text.len() - 1);
        }

        if text_start > 1 {
            text.drain(1..text_start);
        }
    }
}

/// Dedents multi-lined strings
///
/// Multi-lined strings in YANG are practically always indented to match the context. Since we
/// might completely change the indent around strings, we might as well dedent the strings and
/// recalculate the indentation later during formatting.
///
/// This function assumes any strings have already been stripped, see "strip_string".
///
fn dedent_multilined_string(node: &mut Node) {
    let value = if let Some(value) = node.node_value() {
        value
    } else {
        return;
    };

    let text = if let NodeValue::String(text) = value {
        text
    } else {
        return;
    };

    let quotechar = text.chars().next().unwrap();

    // Strips off the quote characters
    let text = &text[1..text.len() - 1];
    let lines: Vec<_> = text.lines().collect();

    if lines.len() < 2 {
        return;
    }

    // The first line is often right at the opening quote, so it doesn't make sense to include it
    // in the text that gets dedented
    let first_line = lines.first().unwrap();

    let rest = lines.get(1..).unwrap().join("\n");
    let rest = textwrap::dedent(&rest);

    let new_text = format!("{}{}\n{}{}", quotechar, first_line, rest, quotechar);

    match node {
        Node::Statement(ref mut node) => node.value = Some(NodeValue::String(new_text)),
        _ => unreachable!("If node isn't a statement, how did we get the mutable value?"),
    };
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
            // Checks if the line will be longer than the configured max width
            //
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
                NodeValue::Other(text) => write_simple_value!(line_pos, text),
                NodeValue::String(text) => {
                    if (text.contains('\n')) {
                        // Multi-lined strings need to be indented
                        writeln!(out)?;
                        indent!(depth + 1);

                        let mut lines = text.lines();

                        // The first line is written normally
                        write!(out, "{}", lines.next().unwrap())?;

                        // Each subsequent non-empty line are indented to match the starting column
                        // of the first line, i.e. right after the quote
                        let extra_indent = config.indent_width() + 1;

                        while let Some(line) = lines.next() {
                            writeln!(out)?;

                            if !line.is_empty() {
                                indent!(depth);

                                for _ in 0..extra_indent {
                                    write!(out, " ")?;
                                }
                            }

                            write!(out, "{}", line)?;
                        }
                    } else {
                        write_simple_value!(line_pos, text);
                    }
                }
                NodeValue::StringConcatenation(concat) => {
                    let kwlen = kw_text.len();
                    let pad = if kwlen >= 2 { kwlen - 2 } else { 0 };

                    // The first string gets written on the same line as the keywords
                    write!(out, " {}", concat[0].0)?;

                    for comment in &concat[0].1 {
                        write!(out, " {}", comment)?;
                    }

                    // The rest get displayed on new lines, padded to align with the first string
                    if let Some(rest) = concat.get(1..) {
                        for (ref string, ref comments) in rest {
                            writeln!(out)?;
                            indent!(depth);

                            for _ in 0..pad {
                                write!(out, " ")?
                            }

                            write!(out, " + {}", string)?;

                            for comment in comments {
                                write!(out, " {}", comment)?;
                            }
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
            indent!(depth);
            write_keyword!(node);

            if node.value.is_some() {
                write_value!(node);
            }

            if let Some(ref children) = node.children {
                write!(out, " {{")?;

                for comment in &node.post_comments {
                    write!(out, " {}", comment)?;
                }

                writeln!(out)?;

                for child in children.as_slice() {
                    write_node(out, child, config, depth + 1)?;
                }

                indent!(depth);
                write!(out, "}}")?;
            } else {
                write!(out, ";")?;

                for comment in &node.post_comments {
                    write!(out, " {}", comment)?;
                }
            }

            write!(out, "\n")?; // All statements implicitly end with a line break
        }

        Node::Comment(text) => {
            indent!(depth);
            writeln!(out, "{text}")?;
        }

        Node::EmptyLine(_) => {
            writeln!(out)?;
        }
    }

    Ok(())
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
            fix_canonical_order: false,
        };

        write_node(&mut out, module_node, &config, 0).expect("Formatting failed");

        let result = String::from_utf8(out).unwrap();

        assert_eq!(
            dedent(
                r#"
                module foo {
                    bar "testing";
                    foo 123.45;


                    revision 2022-02-02 {
                        description "qwerty";
                    }
                    oh "dear";

                }
                "#
            ),
            result,
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
                description "  I should be stripped   ";
                description
                    "
                    I should be stripped and changed to 1 line
                    ";
                description "I am multi-lined,
                    so I automatically get wrapped
                    to the next line even though each
                    individual line is short.";

                description "
                The first line break here should be removed

                     Then the rest of the string should be properly indented.
                     The trailing line breaks should also be removed.

                ";

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

                //
                // Canonical order
                //

                leaf moo {
                    description "I should not be sorted because sorting is not enabled";
                    type string;
                }
                }"#,
            )
            .as_bytes(),
            &(FormatConfig {
                indent: Indent::Spaces(4),
                line_length: 70,
                fix_canonical_order: false,
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
                    description "I should be stripped";
                    description "I should be stripped and changed to 1 line";
                    description
                        "I am multi-lined,
                         so I automatically get wrapped
                         to the next line even though each
                         individual line is short.";

                    description
                        "The first line break here should be removed

                         Then the rest of the string should be properly indented.
                         The trailing line breaks should also be removed.";

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

                    test "foo" { /* foo */ /* bar */ /* baz */ /* pow */
                        // Nobody's ever going to do this (hopefully) so let's not even bother trying
                        // to make it prettier. Just don't crash.
                    }

                    test "foo"; // A comment here is fine
                    test "foo"; /* This however, is not fine*/
                    test "foo"; /* Nobody would ever do this, let's just not crash */ /* yuck */

                    //
                    // Canonical order
                    //

                    leaf moo {
                        description
                            "I should not be sorted because sorting is not enabled";
                        type string;
                    }
                }
                "#
            ),
            result,
        );
    }

    #[test]
    #[ignore]
    fn test_format_with_fix_canonical_order() {
        let result = format_yang_str(
            dedent(
                r#"
                leaf {
                    type string;


                    description "I should be moved to the bottom";

                    must "foo" {
                        // ...
                    }
                }
                "#,
            )
            .as_bytes(),
            &(FormatConfig {
                indent: Indent::Spaces(4),
                line_length: 70,
                fix_canonical_order: true,
            }),
        )
        .unwrap();

        assert_eq!(
            dedent(
                r#"
                leaf {
                    type string;
                    description "I should be moved to the bottom";
                }
                "#
            ),
            result,
        );
    }
}
