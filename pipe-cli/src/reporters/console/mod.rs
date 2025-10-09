use console::{style, Emoji};
use pipe_lib::config::Parameters;
use pipe_lib::{Details, Output, Void};

pub fn run(output: Output, parameters: Option<Parameters>) -> Void {
    eprintln!("\n");
    let mut count = 0;
    for details in output {
        match details {
            | Some(Details {
                condition: false,
                description,
                error,
                ..
            }) => {
                let err = match error {
                    | Some(e) => e.to_string(),
                    | None => "".to_owned(),
                };
                if let Some(msg) = description {
                    if let Some(Parameters { pass: false, minimal, .. }) = parameters {
                        print_fail(msg, err, minimal);
                        count += 1;
                    }
                };
            }
            | Some(Details { description: Some(msg), .. }) => {
                if let Some(Parameters { all: true, minimal, .. }) | Some(Parameters { pass: true, minimal, .. }) = parameters {
                    print_pass(msg, minimal);
                    count += 1;
                };
            }
            | Some(Details { description: None, .. }) => (),
            | None => (),
        }
    }
    if count > 0 {
        eprintln!("\n");
    }
    Ok(())
}

fn print_pass(value: String, minimal: bool) {
    let output = if minimal {
        value
    } else {
        static CHECKMARK: Emoji<'_, '_> = Emoji("✔", ":-)");
        format!("\t{} {}", style(CHECKMARK).green(), value)
    };
    println!("{output}");
}
fn print_fail(value: String, err: String, minimal: bool) {
    let output = if minimal {
        value
    } else {
        const RED_X: Emoji<'_, '_> = Emoji("✘", ":-(");
        format!("\t{} {} {}", style(RED_X).red(), value, style(err).red())
    };
    println!("{output}")
}
