#[macro_use]
extern crate lazy_static;

mod constants;
mod formatting;
mod lexing;
mod parsing;
mod parsing_dbg;

use std::io::{stdin, stdout, Read, Write};

use clap::Parser;

use crate::formatting::{format_yang, FormatConfig, Indent};
use crate::lexing::DebugTokenExt;

/// YANG auto-formatter, inspired by the consistent style of IETF YANG models
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Will try to wrap at this column
    #[arg(short, long, default_value_t = 79)]
    max_width: u16,

    /// Number of spaces used for indentation
    #[arg(short, long, default_value_t = 2)]
    tab_width: u8,

    /// (debugging) Show raw lexer output rather than auto-formatting
    #[arg(long, default_value_t = false)]
    lex: bool,

    /// (debugging) Show the syntax tree rather than auto-formatting
    #[arg(long, default_value_t = false)]
    tree: bool,

    /// Path of the file to format (leave empty or use "-" for STDIN)
    file_path: Option<String>,
}

fn main() {
    let args = Args::parse();

    let config = FormatConfig {
        indent: Indent::Spaces(args.tab_width),
        line_length: args.max_width,
    };

    let mut buffer: Vec<u8> = vec![];

    match args.file_path {
        Some(file_path) => {
            if file_path == "-" {
                read_stdin(&mut buffer)
            } else {
                read_file(&mut buffer, file_path)
            }
        }
        None => read_stdin(&mut buffer),
    }

    let mut stdout = stdout().lock();

    if args.lex {
        for token in crate::lexing::scan(&buffer) {
            writeln!(stdout, "{}", token.human_readable_string())
                .or_error("Failed to write to STDOUT");
        }

        return;
    }

    if args.tree {
        let tree = match crate::parsing::parse(&buffer) {
            Ok(tree) => tree,
            Err(error) => exit_with_error(format!("Failed to parse input file: {error}")),
        };

        if let Err(error) = writeln!(stdout, "{}", tree) {
            exit_with_error(format!("Failed to format tree: {error}"));
        }

        return;
    }

    if let Err(error) = format_yang(&mut stdout, &buffer, &config) {
        exit_with_error(error);
    }
}

fn read_stdin(buffer: &mut Vec<u8>) {
    if let Err(error) = stdin().read_to_end(buffer) {
        exit_with_error(format!("Failed to read from STDIN: {}", error));
    };
}

fn read_file<T: AsRef<str>>(buffer: &mut Vec<u8>, file_path: T) {
    let mut file = match std::fs::File::open(file_path.as_ref()) {
        Ok(file) => file,
        Err(error) => exit_with_error(format!("Failed to open file: {}", error)),
    };

    if let Err(error) = file.read_to_end(buffer) {
        exit_with_error(format!("Failed to read from input file: {}", error));
    }
}

fn exit_with_error<T: std::fmt::Display>(msg: T) -> ! {
    eprintln!("Error: {}", msg);
    std::process::exit(1);
}

trait OrError<T> {
    /// Return the success result or exit the process with an error message
    fn or_error(self, msg: &str) -> T;
}

impl<T, E> OrError<T> for Result<T, E> {
    fn or_error(self, msg: &str) -> T {
        match self {
            Ok(result) => result,
            Err(_) => {
                exit_with_error(msg);
            }
        }
    }
}
