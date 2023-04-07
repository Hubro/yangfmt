use std::fmt::{self, Display, Formatter};

use crate::parsing::{Node, NodeValue, RootNode, StatementKeyword};

pub fn format_tree(out: &mut Formatter, root: &RootNode) -> Result<(), fmt::Error> {
    write!(out, "(root")?;

    for node in root.children.iter() {
        format_node(out, node, 1)?;
    }

    writeln!(out, ")")?;

    Ok(())
}

fn format_node(out: &mut Formatter, node: &Node, depth: u8) -> Result<(), fmt::Error> {
    macro_rules! indent {
        () => {
            for _ in 0..depth {
                write!(out, "  ")?;
            }
        };
    }

    writeln!(out)?;
    indent!();

    match node {
        Node::Leaf(node) => match node.value {
            Some(ref value) => write!(out, "({} {})", node.keyword, value)?,
            _ => write!(out, "({})", node.keyword)?,
        },
        Node::Block(node) => {
            match node.value {
                Some(ref value) => write!(out, "({} {}", node.keyword, value)?,
                None => write!(out, "({}", node.keyword)?,
            }

            for node in node.children.iter() {
                format_node(out, node, depth + 1)?;
            }

            write!(out, ")")?;
        }
        Node::LineBreak(text) => {
            write!(out, "[LineBreak {text:?}]")?;
        }
        Node::Comment(_) => {
            write!(out, "(comment)")?;
        }
    }

    Ok(())
}

impl Display for RootNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        format_tree(f, self)
    }
}

impl Display for StatementKeyword {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            StatementKeyword::Keyword(string) => write!(f, "Keyword {:?}", string)?,
            StatementKeyword::ExtensionKeyword(string) => {
                write!(f, "ExtensionKeyword {:?}", string)?
            }
            StatementKeyword::Invalid(string) => write!(f, "INVALID {:?}", string)?,
        };

        Ok(())
    }
}

impl Display for NodeValue {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            NodeValue::String(_) => write!(f, "String")?,
            NodeValue::StringConcatenation(_) => write!(f, "StringConcatenation")?,
            NodeValue::Number(_) => write!(f, "Number")?,
            NodeValue::Date(_) => write!(f, "Date")?,
            NodeValue::Other(_) => write!(f, "Other")?,
        };

        Ok(())
    }
}
