//! Small manual test harness for `artisan_middleware::notifications::Email`.
//!
//! Usage:
//!   cargo run --example test_email -- <destination> [subject] [body] [server_addr]
//!
//! Examples:
//!   cargo run --example test_email -- someone@example.com
//!   cargo run --example test_email -- someone@example.com "Hi" "Test body" 127.0.0.1:1827

use artisan_middleware::notifications::Email;
use dusa_collection_utils::core::types::stringy::Stringy;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);

    let destination = args
        .next()
        .unwrap_or_else(|| "test@example.com".to_string());
    let subject = args
        .next()
        .unwrap_or_else(|| "Test Subject".to_string());
    let body = args
        .next()
        .unwrap_or_else(|| "Test body from test_email example".to_string());
    let server_addr = args.next();

    let email = Email::new(
        Stringy::from(destination),
        Stringy::from(subject),
        Stringy::from(body),
    );

    println!("Constructed: {}", email);
    println!("is_valid(): {}", email.is_valid());

    let json = email.to_json().expect("failed to serialize email");
    println!("to_json(): {}", json);

    let round_tripped = Email::from_json(&json).expect("failed to deserialize email");
    println!("from_json() round trip: {}", round_tripped);
    assert_eq!(email.destination, round_tripped.destination);
    assert_eq!(email.subject, round_tripped.subject);
    assert_eq!(email.body, round_tripped.body);
    println!("Round trip OK.");

    println!(
        "Sending to {}...",
        server_addr.as_deref().unwrap_or("default MAIL_ADDRESS")
    );
    match email.send(server_addr.as_deref()).await {
        Ok(()) => println!("Email sent successfully!"),
        Err(err) => eprintln!("Failed to send email: {err}"),
    }
}
