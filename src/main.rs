mod parser;

use parser::JournalBackupReader;
use std::io::{self, ErrorKind, Write};
use structopt::StructOpt;

/// This doc string acts as a help message when the user runs '--help'
/// as do all doc strings on fields
#[derive(StructOpt, Debug, Clone)]
#[structopt(version = "0.1.0", author = "Kevin Nisbet <kevin@xybyte.com>")]
struct Opts {
    #[structopt(flatten)]
    filter: parser::Filter,

    /// Journal export files to parse
    files: Vec<String>,

    /// Change journal output mode
    #[structopt(short, long = "output", possible_values = &parser::OutputMode::variants(), case_insensitive = true)]
    pub output_mode: Option<parser::OutputMode>,
}

fn main() {
    let opts: Opts = Opts::from_args();
    //println!("{:?}", opts);

    let mut line_count = 0;

    for file in opts.clone().files {
        for msg in JournalBackupReader::open_file(file, Some(opts.filter.clone())).unwrap() {
            if let Err(e) = io::stdout().write_all(msg.to_string(opts.clone().output_mode).as_bytes()) {
                match e.kind() {
                    ErrorKind::BrokenPipe => return,
                    _ => {
                        eprintln!("write to stdout failed: {:?}", e);
                    }
                }
            }

            line_count+=1;
            if let Some(line_limit) = opts.filter.clone().lines {
                if line_count == line_limit {
                    return
                }
            }
        }
    }
}
