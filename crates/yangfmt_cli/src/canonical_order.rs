#![allow(dead_code)]
/// This module handles sorting statements into the canonical order described by the ABNF.
///
/// Since this code formatter is designed to be used while editing (for example on-save) it has to
/// strike a balance between strictness and friendliness. If a user writes out a line, hits save
/// and the line disappears from the screen, that's a pretty bad user experience.
///
/// For that reason, this module will attempt to only auto-sort lines that are almost certain to be
/// on the same screen at any point in time, such as statements within leaf blocks or at the top of
/// module blocks.
///
/// Note: I can't imagine a way to properly handle empty lines when sorting a list of statements,
/// so if a statement list needs to be sorted, any empty lines will be removed. This is an area for
/// possible improvement if I can think of a good strategy.
///
use phf::phf_map;

use crate::parsing::Node;

type OrderMapping = phf::Map<&'static str, u8>;

/// Describes the canonical order of statements inside a leaf or leaf-list block.
static LEAF_CANONICAL_ORDER: OrderMapping = phf_map! {
    "when" => 1,
    "if-feature" => 2,
    "type" => 3,
    "units" => 4,
    "must" => 5,
    "default" => 6,
    "config" => 7,
    "min-elements" => 8,
    "max-elements" => 9,
    "ordered-by" => 10,
    "mandatory" => 11,
    "status" => 12,
    "description" => 13,
    "reference" => 14,
};

/// Checks if all the statements in the statement list is sorted
///
/// This ignores line breaks and comments.
///
pub fn is_sorted(order_mapping: &OrderMapping, statements: &mut Vec<Node>) -> bool {
    let mut previous: Option<u8> = None;

    for statement in statements.iter().take(statements.len() - 1) {
        match statement {
            Node::Statement(statement) => {
                let sort_value = match order_mapping.get(statement.keyword.text()) {
                    Some(value) => *value,
                    None => u8::MAX,
                };

                match previous {
                    None => previous = Some(sort_value),
                    Some(previous) => {
                        if sort_value < previous {
                            return false;
                        }
                    }
                }
            }
            _ => continue, // Ignore comments and empty lines
        }
    }

    true
}

/// Sorts the input statement list following the canonical order from the ABNF
pub fn sort_statements(_parent_node_name: Option<&str>, _statements: &mut [Node]) {
    // match parent_node_name {
    //     Some("leaf") => sort_statements_with(&LEAF_CANONICAL_ORDER, statements),
    //     Some(_) => (),
    //     None => (),
    // }
}

fn sort_statements_with(order_mapping: &OrderMapping, statements: &mut [Node]) {
    statements.sort_by(|node_a, node_b| {
        get_order_for(order_mapping, node_a).cmp(&get_order_for(order_mapping, node_b))
    })
}

fn get_order_for(order_mapping: &OrderMapping, node: &Node) -> u8 {
    match node {
        Node::Statement(statement) => match order_mapping.get(statement.keyword.text()) {
            Some(order) => *order,
            None => u8::MAX,
        },
        _ => u8::MAX,
    }
}
