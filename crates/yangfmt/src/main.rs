use std::io::{stdin, stdout, Read, Write};

use clap::Parser;

use yangfmt_formatting::{format_yang, Error as FormattingError, FormatConfig, Indent};
use yangfmt_lexing::DebugTokenExt;

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

    /// Sort statements to match canonical order
    #[arg(short, long, default_value_t = false)]
    canonical_order: bool,

    /// Format the file in-place rather than print to STDOUT (use with caution!)
    #[arg(short, long, default_value_t = false, requires("file_path"))]
    in_place: bool,

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
        fix_canonical_order: args.canonical_order,
    };

    let mut buffer: Vec<u8> = vec![];

    // Check that "-i" and file path "-" isn't provided at the same time
    if args.file_path.as_ref().map_or(false, |path| path == "-") && args.in_place {
        exit_with_error("Can't modify STDIN in place");
    }

    match args.file_path {
        Some(ref file_path) => {
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
        for token in yangfmt_lexing::scan_iter(&buffer) {
            match token {
                Ok(token) => writeln!(stdout, "{}", token.human_readable_string())
                    .or_error("Failed to write to STDOUT"),
                Err(error) => exit_with_error(format!("Lexer error: {error:?}")),
            }
        }

        return;
    }

    if args.tree {
        let tree = match yangfmt_parsing::parse(&buffer) {
            Ok(tree) => tree,
            Err(error) => exit_with_error(format!("Failed to parse input file: {error:?}")),
        };

        if let Err(error) = writeln!(stdout, "{}", tree) {
            exit_with_error(format!("Failed to format tree: {error}"));
        }

        return;
    }

    if args.in_place {
        let file_path = args.file_path.as_ref().unwrap();
        let mut output_buffer: Vec<u8> = vec![];

        if let Err(error) = format_yang(&mut output_buffer, &buffer, &config) {
            handle_formatting_error(error, &buffer);
        }

        if let Err(error) = std::fs::write(file_path, output_buffer) {
            exit_with_error(error);
        }
    }

    if !args.in_place {
        if let Err(error) = format_yang(&mut stdout, &buffer, &config) {
            handle_formatting_error(error, &buffer);
        }
    }
}

fn handle_formatting_error(error: FormattingError, buffer: &[u8]) {
    match error {
        FormattingError::ParseError(parse_error) => {
            let pos = TextPosition::from_buffer_index(buffer, parse_error.position);
            exit_with_error(format!("Parse error at {}: {}", pos, parse_error.message));
        }
        FormattingError::IOError(error) => exit_with_error(error),
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

/// 1-based cursor position in a text file
pub struct TextPosition {
    line: usize,
    col: usize,
}

impl TextPosition {
    pub fn from_buffer_index(buffer: &[u8], index: usize) -> Self {
        let mut line = 1;
        let mut col = 1;

        for (i, c) in buffer.iter().enumerate() {
            if i == index {
                break;
            }

            if *c == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        Self { line, col }
    }
}

impl core::fmt::Display for TextPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {} col {}", self.line, self.col)
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
