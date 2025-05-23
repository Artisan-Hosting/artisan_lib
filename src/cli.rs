use std::io::{self, Write};

use dusa_collection_utils::{core::errors::ErrorArrayItem, core::types::stringy::Stringy};

use crate::encryption::simple_encrypt;

/// Capture user input from the terminal
/// Returns a `Stringy` item after printing the prompt
/// `message: `
pub fn get_user_input(prompt: &str) -> Stringy {
    print!("{}: ", prompt); // Print the prompt message
    io::stdout().flush().unwrap(); // Make sure the prompt is printed before user input

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input");
    Stringy::from(input.trim()) // Remove any trailing newline or spaces
}

/// Capture user input from the terminal and encrypts it
pub async fn get_encrypted_user_input(prompt: &str) -> Result<Stringy, ErrorArrayItem> {
    let user_input: Stringy = get_user_input(prompt);
    // let data = encrypt_text(user_input).await;
    let data = simple_encrypt(user_input.as_bytes())?;
    Ok(data)
}

/// Display options and capture the user's selection
pub fn get_user_selection(options: &[String]) -> usize {
    // Display the options to the user
    for (i, option) in options.iter().enumerate() {
        println!("{}. {}", i + 1, option);
    }

    loop {
        let input = get_user_input("Please enter the number of your choice: ");

        // Try to parse the input as a number
        match input.parse::<usize>() {
            Ok(choice) if choice > 0 && choice <= options.len() => return choice,
            _ => println!("Invalid choice, please try again."),
        }
    }
}

/// Ask the user for a Yes/No confirmation
pub fn get_yes_no(prompt: &str) -> bool {
    loop {
        let input = get_user_input(&format!("{} (y/n): ", prompt));
        match input.to_lowercase().as_str() {
            "y" | "yes" => return true,
            "n" | "no" => return false,
            _ => println!("Invalid input, please enter 'y' or 'n'."),
        }
    }
}

pub fn clean_screen() {
    // Clearing screen
    print!("\x1B[2J\x1B[H");
}
