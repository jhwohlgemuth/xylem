use std::env::args;
use std::fs::{read_to_string, write};
use std::process::ExitCode;

fn main() -> ExitCode {
    match args().skip(1).collect::<Vec<_>>().as_slice() {
        | [input, output] => match read_to_string(input) {
            | Ok(value) => {
                let result = format!("binary: {value}");
                match write(output, &result) {
                    | Ok(()) => {
                        print!("{result}");
                        ExitCode::SUCCESS
                    }
                    | Err(why) => {
                        eprintln!("write {output}: {why}");
                        ExitCode::FAILURE
                    }
                }
            }
            | Err(why) => {
                eprintln!("read {input}: {why}");
                ExitCode::FAILURE
            }
        },
        | _ => {
            eprintln!("usage: finalize <input> <output>");
            ExitCode::FAILURE
        }
    }
}
