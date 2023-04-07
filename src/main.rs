#![allow(dead_code, unused_imports, unused_variables, unused_macros)]

#[macro_use]
extern crate lazy_static;

mod constants;
mod formatting;
mod lexing;
mod parsing;

#[cfg(test)]
mod parsing_dbg;

use std::io::{stdin, stdout, Read};

use formatting::Indent;

use crate::formatting::{format_yang, FormatConfig};

fn main() {
    let config = FormatConfig {
        indent: Indent::Spaces(4),
        line_length: 80,
    };

    let mut buffer: Vec<u8> = vec![];

    stdin()
        .read_to_end(&mut buffer)
        .expect("Failed to read STDIN");

    let result = format_yang(&mut stdout().lock(), &buffer, &config);

    if let Err(err) = result {
        let msg = err.to_string();

        if msg.contains("Broken pipe") {
            return; // This is fine, silently ignore
        } else {
            panic!("{msg}");
        }
    }
}
