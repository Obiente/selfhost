//! Shared terminal input for human workflows. Backend plans remain the authority.
use anyhow::{Result, ensure};
use serde_json::Value;
use std::io::{self, IsTerminal, Write};

pub fn interactive() -> Result<()> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "Run this guided command in an interactive terminal. Use explicit inputs for automation."
    );
    Ok(())
}

pub fn input(label: &str, default: &str) -> Result<String> {
    interactive()?;
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
    io::stdout().flush()?;
    let mut answer = String::new();
    ensure!(io::stdin().read_line(&mut answer)? > 0, "Cancelled");
    ensure!(answer.len() <= 8192, "Answer is too long");
    let answer = answer.trim();
    ensure!(
        !answer.chars().any(char::is_control),
        "Control characters are not allowed"
    );
    Ok(if answer.is_empty() {
        default.into()
    } else {
        answer.into()
    })
}

pub fn confirm(label: &str, default: bool) -> Result<bool> {
    loop {
        let value = input(
            &format!("{label} {}", if default { "[Y/n]" } else { "[y/N]" }),
            "",
        )?;
        match value.to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Please answer y or n."),
        }
    }
}

pub fn choose(label: &str, choices: &[String]) -> Result<usize> {
    ensure!(!choices.is_empty(), "No available choices for {label}");
    println!("\n{label}");
    for (i, choice) in choices.iter().enumerate() {
        println!("  {}. {choice}", i + 1);
    }
    loop {
        let answer = input("Choose a number (q to cancel)", "")?;
        ensure!(!answer.eq_ignore_ascii_case("q"), "Cancelled");
        if let Some(index) = choice_index(&answer, choices) {
            return Ok(index);
        }
        println!("Choose a number between 1 and {}.", choices.len());
    }
}
fn choice_index(answer: &str, choices: &[String]) -> Option<usize> {
    answer
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0 && *n <= choices.len())
        .map(|n| n - 1)
        .or_else(|| {
            choices
                .iter()
                .position(|label| label.eq_ignore_ascii_case(answer))
        })
}

pub fn secret(label: &str) -> Result<String> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
        terminal,
    };
    interactive()?;
    print!("{label}: ");
    io::stdout().flush()?;
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = terminal::disable_raw_mode();
            let _ = crossterm::execute!(io::stdout(), event::DisableBracketedPaste);
        }
    }
    terminal::enable_raw_mode()?;
    let guard = Guard;
    crossterm::execute!(io::stdout(), event::EnableBracketedPaste)?;
    let mut value = String::new();
    loop {
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Enter => break,
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    anyhow::bail!("Cancelled")
                }
                KeyCode::Esc => anyhow::bail!("Cancelled"),
                KeyCode::Char(c) if !c.is_control() => value.push(c),
                _ => {}
            },
            Event::Paste(text) => value.push_str(text.trim()),
            _ => {}
        }
        ensure!(value.len() <= 128 * 1024, "Secret input is too long");
    }
    drop(guard);
    println!();
    Ok(value)
}

/// Display already-redacted backend plans, keeping approval machinery internal.
pub fn review(value: &Value) {
    println!("\nReview:");
    print!("{}", review_text(value, 1));
}
fn review_text(value: &Value, depth: usize) -> String {
    let indent = "  ".repeat(depth);
    match value {
        Value::Object(map) => map
            .iter()
            .filter(|(key, _)| {
                !matches!(
                    key.as_str(),
                    "revision"
                        | "expected_revision"
                        | "confirmation"
                        | "plan_id"
                        | "target_revision"
                        | "password"
                        | "client_secret"
                        | "api_token"
                        | "credential"
                        | "token"
                        | "private_key"
                )
            })
            .map(|(key, value)| {
                let label = key.replace('_', " ");
                match value {
                    Value::Null => String::new(),
                    Value::Object(_) | Value::Array(_) => {
                        format!("{indent}{label}:\n{}", review_text(value, depth + 1))
                    }
                    Value::String(text) => format!("{indent}{label}: {}\n", printable(text)),
                    _ => format!("{indent}{label}: {value}\n"),
                }
            })
            .collect(),
        Value::Array(items) => items.iter().map(|item| review_text(item, depth)).collect(),
        Value::String(text) => format!("{indent}- {}\n", printable(text)),
        Value::Null => String::new(),
        _ => format!("{indent}- {value}\n"),
    }
}
fn printable(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn choices_are_bounded_and_review_omits_approval_and_secret_fields() {
        let options = vec!["First".into(), "Second".into()];
        assert_eq!(choice_index("0", &options), None);
        assert_eq!(choice_index("3", &options), None);
        assert_eq!(choice_index("2", &options), Some(1));
        assert_eq!(choice_index("second", &options), Some(1));
        let text = review_text(
            &serde_json::json!({"revision":"private-revision","client_secret":"secret-value","changes":[{"name":"Example","after":true}],"warnings":["Review the service"]}),
            1,
        );
        assert!(!text.contains("private-revision") && !text.contains("secret-value"));
        assert!(text.contains("Example") && text.contains("Review the service"));
    }
}
