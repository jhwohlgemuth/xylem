use pipe_lib::add_forward_slash as _add_forward_slash;
use pyo3::prelude::*;
use rand::Rng;
use std::cmp::Ordering;
use std::{io, path::PathBuf};

#[pyfunction]
fn add_forward_slash(value: &str) -> String {
    let path = PathBuf::from(value);
    _add_forward_slash(path).into_os_string().into_string().unwrap()
}

#[pyfunction]
fn guess_the_number() {
    println!("Guess the number!");
    let secret_number = rand::thread_rng().gen_range(1..101);
    loop {
        println!("Please input your guess.");
        let mut guess = String::new();
        io::stdin().read_line(&mut guess).expect("Failed to read line");
        let guess: u32 = match guess.trim().parse() {
            | Ok(num) => num,
            | Err(_) => continue,
        };
        println!("You guessed: {}", guess);
        match guess.cmp(&secret_number) {
            | Ordering::Less => println!("Too small!"),
            | Ordering::Greater => println!("Too big!"),
            | Ordering::Equal => {
                println!("You win!");
                break;
            }
        }
    }
}

#[pymodule]
fn pipe(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(add_forward_slash, m)?)?;
    m.add_function(wrap_pyfunction!(guess_the_number, m)?)?;
    Ok(())
}
