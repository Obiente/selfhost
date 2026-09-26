use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    future::Future,
    io::{self, Read, Write},
    time::Duration,
};
use unicode_width::UnicodeWidthStr;

pub struct Terminal;
impl Terminal {
    pub fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let guard = Self;
        execute!(
            io::stdout(),
            terminal::EnterAlternateScreen,
            event::EnableBracketedPaste,
            cursor::Hide
        )?;
        Ok(guard)
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            event::DisableBracketedPaste,
            terminal::LeaveAlternateScreen,
            cursor::Show,
            ResetColor
        );
    }
}
fn unsafe_format(c: char) -> bool {
    matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{206f}'|'\u{feff}')
}
pub fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() && !unsafe_format(*c))
        .collect()
}
fn clipped(text: &str, width: usize) -> String {
    let mut result = String::new();
    for c in clean(text).chars() {
        result.push(c);
        if result.width() > width {
            result.pop();
            break;
        }
    }
    result
}
fn display_chars(chars: &[char], secret: bool) -> String {
    chars
        .iter()
        .map(|c| {
            if secret {
                '*'
            } else if *c == '\t' {
                ' '
            } else {
                *c
            }
        })
        .collect()
}
fn visible_start(chars: &[char], position: usize, columns: usize, secret: bool) -> usize {
    let mut start = position;
    while start > 0 {
        if clean(&display_chars(&chars[start - 1..position], secret)).width() >= columns {
            break;
        }
        start -= 1;
    }
    start
}
fn move_cursor(x: usize, y: usize) -> Result<()> {
    let (w, h) = dimensions()?;
    execute!(
        io::stdout(),
        cursor::MoveTo(
            x.min(w.saturating_sub(1) as usize) as u16,
            y.min(h.saturating_sub(1) as usize) as u16
        ),
        cursor::Show
    )?;
    Ok(())
}
fn dimensions() -> Result<(u16, u16)> {
    Ok(terminal::size()?)
}
fn line(y: u16, text: &str, selected: bool) -> Result<()> {
    let (width, height) = dimensions()?;
    if y >= height {
        return Ok(());
    }
    execute!(
        io::stdout(),
        cursor::MoveTo(2.min(width.saturating_sub(1)), y),
        SetForegroundColor(if selected {
            Color::Rgb {
                r: 216,
                g: 184,
                b: 112,
            }
        } else {
            Color::Reset
        }),
        Print(clipped(text, width.saturating_sub(4) as usize)),
        ResetColor
    )?;
    Ok(())
}
fn frame(title: &str, help: &str) -> Result<usize> {
    let (_, height) = dimensions()?;
    execute!(io::stdout(), cursor::Hide, Clear(ClearType::All))?;
    line(1, &format!("selfhost / {title}"), true)?;
    line(height.saturating_sub(2), help, false)?;
    Ok(height.saturating_sub(7).max(1) as usize)
}
fn flush() -> Result<()> {
    io::stdout().flush()?;
    Ok(())
}
fn read() -> Result<Event> {
    loop {
        let e = event::read()?;
        if let Event::Key(key) = &e
            && key.kind == KeyEventKind::Release
        {
            continue;
        }
        return Ok(e);
    }
}
fn cancel(key: &KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
}

/// Filtered menu indexes always refer to the caller's original collection.
pub fn filtered(items: &[String], query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    items
        .iter()
        .enumerate()
        .filter(|(_, v)| v.to_lowercase().contains(&query))
        .map(|(i, _)| i)
        .collect()
}
pub fn select(title: &str, items: &[String]) -> Result<Option<usize>> {
    let mut selected = 0usize;
    let mut query = String::new();
    loop {
        let matches = filtered(items, &query);
        selected = selected.min(matches.len().saturating_sub(1));
        let capacity = frame(
            title,
            "Up/Down select | Enter open | Type to filter | Esc back",
        )?;
        line(
            3,
            &format!("Filter: {query}   {} matches", matches.len()),
            false,
        )?;
        let start = selected.saturating_sub(capacity.saturating_sub(1));
        for (row, index) in matches.iter().enumerate().skip(start).take(capacity) {
            line(
                5 + (row - start) as u16,
                &format!(
                    "{} {}",
                    if row == selected { ">" } else { " " },
                    items[*index]
                ),
                row == selected,
            )?;
        }
        if matches.is_empty() {
            line(
                5,
                "No matching items. Clear the filter or press Esc.",
                false,
            )?;
        }
        flush()?;
        if let Event::Key(key) = read()? {
            if cancel(&key) {
                return Ok(None);
            }
            match key.code {
                KeyCode::Up => selected = selected.saturating_sub(1),
                KeyCode::Down => selected = (selected + 1).min(matches.len().saturating_sub(1)),
                KeyCode::PageUp => selected = selected.saturating_sub(capacity),
                KeyCode::PageDown => {
                    selected = (selected + capacity).min(matches.len().saturating_sub(1))
                }
                KeyCode::Home => selected = 0,
                KeyCode::End => selected = matches.len().saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(index) = matches.get(selected) {
                        return Ok(Some(*index));
                    }
                }
                KeyCode::Backspace => {
                    query.pop();
                    selected = 0;
                }
                KeyCode::Char(c) if !c.is_control() && query.len() < 128 => {
                    query.push(c);
                    selected = 0;
                }
                _ => {}
            }
        }
    }
}
pub fn menu(title: &str, items: &[&str]) -> Result<Option<usize>> {
    select(
        title,
        &items.iter().map(|v| (*v).into()).collect::<Vec<_>>(),
    )
}
pub fn multi(title: &str, items: &[String], defaults: &[usize]) -> Result<Option<Vec<usize>>> {
    let mut selected = 0usize;
    let mut chosen = defaults.to_vec();
    loop {
        let capacity = frame(
            title,
            "Up/Down select | Space toggle | Enter accept | Esc cancel",
        )?;
        line(3, &format!("{} selected", chosen.len()), false)?;
        let start = selected.saturating_sub(capacity.saturating_sub(1));
        for (i, item) in items.iter().enumerate().skip(start).take(capacity) {
            line(
                5 + (i - start) as u16,
                &format!(
                    "{} [{}] {item}",
                    if i == selected { ">" } else { " " },
                    if chosen.contains(&i) { "x" } else { " " }
                ),
                i == selected,
            )?;
        }
        flush()?;
        if let Event::Key(key) = read()? {
            if cancel(&key) {
                return Ok(None);
            }
            match key.code {
                KeyCode::Up => selected = selected.saturating_sub(1),
                KeyCode::Down => selected = (selected + 1).min(items.len().saturating_sub(1)),
                KeyCode::Char(' ') if selected < items.len() => {
                    if chosen.contains(&selected) {
                        chosen.retain(|i| *i != selected)
                    } else {
                        chosen.push(selected)
                    }
                }
                KeyCode::Enter => return Ok(Some(chosen)),
                _ => {}
            }
        }
    }
}
pub fn yes(title: &str) -> Result<bool> {
    if title.width() > dimensions()?.0.saturating_sub(18) as usize {
        view(
            "Review before continuing",
            &wrapped(title, dimensions()?.0.saturating_sub(6).max(12) as usize),
        )?;
        return Ok(menu(
            "Continue with the reviewed action?",
            &["No, go back", "Yes, continue"],
        )? == Some(1));
    }
    Ok(menu(title, &["No, go back", "Yes, continue"])? == Some(1))
}
fn wrapped(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut column = 0;
    for word in clean(text).split_whitespace() {
        let len = word.width();
        if column > 0 && column + 1 + len > width {
            result.push('\n');
            column = 0;
        } else if column > 0 {
            result.push(' ');
            column += 1;
        }
        result.push_str(word);
        column += len;
    }
    result
}
pub fn prompt(title: &str, help: &str, default: &str, secret: bool) -> Result<Option<String>> {
    let mut value: Vec<char> = default.chars().collect();
    let mut position = value.len();
    loop {
        frame(
            title,
            "Enter save | Esc cancel | Left/Right edit | Ctrl+U clear",
        )?;
        line(3, help, false)?;
        let (width, _) = dimensions()?;
        let capacity = width.saturating_sub(6).max(1) as usize;
        let offset = visible_start(&value, position, capacity, secret);
        let rendered = clipped(&display_chars(&value[offset..], secret), capacity);
        line(5, &rendered, true)?;
        move_cursor(
            2 + clean(&display_chars(&value[offset..position], secret)).width(),
            5,
        )?;
        flush()?;
        match read()? {
            Event::Paste(text) => {
                for c in text
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(8192usize.saturating_sub(value.len()))
                {
                    value.insert(position, c);
                    position += 1;
                }
            }
            Event::Key(key) => {
                if cancel(&key) {
                    return Ok(None);
                }
                match key.code {
                    KeyCode::Enter => return Ok(Some(value.into_iter().collect())),
                    KeyCode::Left => position = position.saturating_sub(1),
                    KeyCode::Right => position = (position + 1).min(value.len()),
                    KeyCode::Home => position = 0,
                    KeyCode::End => position = value.len(),
                    KeyCode::Backspace if position > 0 => {
                        position -= 1;
                        value.remove(position);
                    }
                    KeyCode::Delete if position < value.len() => {
                        value.remove(position);
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        value.clear();
                        position = 0;
                    }
                    KeyCode::Char(c)
                        if !c.is_control()
                            && !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                            && value.len() < 8192 =>
                    {
                        value.insert(position, c);
                        position += 1;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
pub fn required(title: &str, default: &str) -> Result<Option<String>> {
    loop {
        let Some(value) = prompt(title, "Required", default, false)? else {
            return Ok(None);
        };
        if !value.trim().is_empty() {
            return Ok(Some(value));
        }
        view("Enter a value", "This field cannot be empty.")?;
    }
}
pub fn confirm_name(title: &str, name: &str) -> Result<bool> {
    if title.width() > dimensions()?.0.saturating_sub(18) as usize
        || name.width() + 21 > dimensions()?.0.saturating_sub(4) as usize
    {
        view(
            "Review confirmation",
            &format!(
                "{}\n\nExact name (spacing matters):\n{name}",
                wrapped(title, dimensions()?.0.saturating_sub(6).max(12) as usize)
            ),
        )?;
    }
    Ok(
        prompt(title, &format!("Type the exact name: {name}"), "", false)?
            .is_some_and(|v| v == name),
    )
}

pub fn view(title: &str, text: &str) -> Result<()> {
    let lines: Vec<String> = text.lines().map(clean).collect();
    let mut scroll = 0usize;
    let mut horizontal = 0usize;
    loop {
        let capacity = frame(
            title,
            "Up/Down/PgUp/PgDn scroll | Left/Right pan | Esc or Enter back",
        )?;
        scroll = scroll.min(lines.len().saturating_sub(capacity));
        line(
            3,
            &format!(
                "Lines {}-{} of {}",
                scroll + 1,
                (scroll + capacity).min(lines.len()),
                lines.len()
            ),
            false,
        )?;
        for (i, line_text) in lines.iter().skip(scroll).take(capacity).enumerate() {
            line(
                5 + i as u16,
                &line_text.chars().skip(horizontal).collect::<String>(),
                false,
            )?;
        }
        flush()?;
        if let Event::Key(key) = read()? {
            if cancel(&key) || key.code == KeyCode::Enter {
                return Ok(());
            }
            match key.code {
                KeyCode::Up => scroll = scroll.saturating_sub(1),
                KeyCode::Down => scroll = scroll.saturating_add(1),
                KeyCode::PageUp => scroll = scroll.saturating_sub(capacity),
                KeyCode::PageDown => scroll = scroll.saturating_add(capacity),
                KeyCode::Home => {
                    scroll = 0;
                    horizontal = 0;
                }
                KeyCode::End => scroll = lines.len(),
                KeyCode::Left => horizontal = horizontal.saturating_sub(8),
                KeyCode::Right => horizontal = horizontal.saturating_add(8),
                _ => {}
            }
        }
    }
}
pub fn show(title: &str, value: &impl Serialize) -> Result<()> {
    view(title, &serde_json::to_string_pretty(value)?)
}
pub fn review(title: &str, value: &impl Serialize) -> Result<bool> {
    show(title, value)?;
    Ok(menu(
        title,
        &["Keep current settings", "Apply the reviewed changes"],
    )? == Some(1))
}

/// In-terminal editor: no shell, temporary secret files, or external editor invocation.
pub fn editor(title: &str, initial: &str) -> Result<Option<String>> {
    let mut rows: Vec<Vec<char>> = initial
        .split('\n')
        .map(|s| {
            s.trim_end_matches('\r')
                .chars()
                .filter(|c| !c.is_control() || *c == '\t')
                .collect()
        })
        .collect();
    if rows.is_empty() {
        rows.push(vec![])
    }
    let (mut y, mut x) = (0usize, 0usize);
    loop {
        let capacity = frame(
            title,
            "Ctrl+S accept | Enter new line | Arrows edit | Esc discard",
        )?;
        let (width, _) = dimensions()?;
        let visible = width.saturating_sub(8).max(1) as usize;
        let start = y.saturating_sub(capacity.saturating_sub(1));
        let horizontal = visible_start(&rows[y], x, visible, false);
        line(
            3,
            &format!(
                "Line {} column {} | May contain secrets; keep your terminal private",
                y + 1,
                x + 1
            ),
            false,
        )?;
        for (i, row) in rows.iter().enumerate().skip(start).take(capacity) {
            line(
                5 + (i - start) as u16,
                &format!(
                    "{} {}",
                    if i == y { ">" } else { " " },
                    clipped(
                        &display_chars(&row[horizontal.min(row.len())..], false),
                        visible
                    )
                ),
                i == y,
            )?;
        }
        move_cursor(
            4 + clean(&display_chars(&rows[y][horizontal..x], false)).width(),
            5 + y - start,
        )?;
        flush()?;
        match read()? {
            Event::Paste(text) => {
                if rows.iter().map(Vec::len).sum::<usize>() + text.len() > 1024 * 1024 {
                    continue;
                }
                for c in text.chars() {
                    if c == '\n' {
                        let tail = rows[y].split_off(x);
                        y += 1;
                        rows.insert(y, tail);
                        x = 0;
                    } else if !c.is_control() || c == '\t' {
                        rows[y].insert(x, c);
                        x += 1;
                    }
                }
            }
            Event::Key(key) => {
                if cancel(&key) {
                    return Ok(None);
                }
                match key.code {
                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(Some(
                            rows.iter()
                                .map(|r| r.iter().collect::<String>())
                                .collect::<Vec<_>>()
                                .join("\n"),
                        ));
                    }
                    KeyCode::Up => {
                        y = y.saturating_sub(1);
                        x = x.min(rows[y].len());
                    }
                    KeyCode::Down => {
                        y = (y + 1).min(rows.len() - 1);
                        x = x.min(rows[y].len());
                    }
                    KeyCode::Left => {
                        if x > 0 {
                            x -= 1
                        } else if y > 0 {
                            y -= 1;
                            x = rows[y].len()
                        }
                    }
                    KeyCode::Right => {
                        if x < rows[y].len() {
                            x += 1
                        } else if y + 1 < rows.len() {
                            y += 1;
                            x = 0
                        }
                    }
                    KeyCode::Home => x = 0,
                    KeyCode::End => x = rows[y].len(),
                    KeyCode::Enter => {
                        let tail = rows[y].split_off(x);
                        y += 1;
                        rows.insert(y, tail);
                        x = 0;
                    }
                    KeyCode::Backspace => {
                        if x > 0 {
                            x -= 1;
                            rows[y].remove(x);
                        } else if y > 0 {
                            let row = rows.remove(y);
                            y -= 1;
                            x = rows[y].len();
                            rows[y].extend(row);
                        }
                    }
                    KeyCode::Delete => {
                        if x < rows[y].len() {
                            rows[y].remove(x);
                        } else if y + 1 < rows.len() {
                            let row = rows.remove(y + 1);
                            rows[y].extend(row);
                        }
                    }
                    KeyCode::Char(c)
                        if !c.is_control()
                            && !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        rows[y].insert(x, c);
                        x += 1;
                    }
                    KeyCode::Tab => {
                        rows[y].splice(x..x, [' ', ' ']);
                        x += 2;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
pub fn json_edit<T: Serialize + DeserializeOwned>(title: &str, value: &T) -> Result<Option<T>> {
    let mut text = serde_json::to_string_pretty(value)?;
    loop {
        let Some(edited) = editor(title, &text)? else {
            return Ok(None);
        };
        match serde_json::from_str(&edited) {
            Ok(value) => return Ok(Some(value)),
            Err(e) => {
                view("Invalid JSON", &e.to_string())?;
                text = edited;
            }
        }
    }
}
async fn wait_with_ui<T>(
    future: impl Future<Output = Result<T>>,
    mut update: impl FnMut() -> Result<()>,
) -> Result<T> {
    tokio::pin!(future);
    let mut interval = tokio::time::interval(Duration::from_millis(120));
    loop {
        tokio::select! {result=&mut future=>return result,_=interval.tick()=>{if update().is_err(){return future.await;}}}
    }
}
pub async fn wait<T>(title: &str, future: impl Future<Output = Result<T>>) -> Result<T> {
    let mut tick = 0usize;
    wait_with_ui(future, || {
        frame(
            title,
            "Operation in progress. Changes finish before navigation resumes.",
        )?;
        line(
            4,
            &format!("{} Working...", ["|", "/", "-", "\\"][tick % 4]),
            true,
        )?;
        flush()?;
        tick = tick.wrapping_add(1);
        // A continuous key stream must not starve the operation future.
        for _ in 0..64 {
            if !event::poll(Duration::ZERO)? {
                break;
            }
            let _ = event::read()?;
        }
        Ok(())
    })
    .await
}
pub fn number(title: &str, default: u64) -> Result<Option<u64>> {
    let mut current = default.to_string();
    loop {
        let Some(text) = prompt(title, "Whole number", &current, false)? else {
            return Ok(None);
        };
        match text.parse() {
            Ok(n) => return Ok(Some(n)),
            Err(_) => {
                view("Invalid number", "Enter a positive whole number.")?;
                current = text;
            }
        }
    }
}
pub fn load_json<T: DeserializeOwned>(title: &str) -> Result<Option<T>> {
    let Some(path) = required(title, "")? else {
        return Ok(None);
    };
    let file = std::fs::File::open(path).context("Could not read that JSON file")?;
    anyhow::ensure!(file.metadata()?.is_file(), "Choose a regular JSON file");
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= 1024 * 1024, "JSON file exceeds 1 MiB");
    Ok(Some(
        serde_json::from_slice(&bytes).context("Invalid JSON file")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_output_cannot_inject_escape_sequences() {
        assert_eq!(clean("hello\x1b[2J\rworld\x07"), "hello[2Jworld");
    }
    #[test]
    fn filtered_menu_retains_source_indexes() {
        assert_eq!(
            filtered(
                &["Files".into(), "Database".into(), "File browser".into()],
                "FILE"
            ),
            vec![0, 2]
        );
    }
}

#[cfg(test)]
mod safety_tests {
    use super::*;
    #[test]
    fn bidi_controls_and_wide_output_cannot_escape_the_review_column() {
        assert_eq!(clean("safe\u{202e}txt\u{2069}"), "safetxt");
        assert_eq!(clipped("中文x", 3), "中");
        assert_eq!(clipped("🔒x", 2), "🔒");
        assert_eq!(clipped("e\u{301}x", 1), "e\u{301}");
        let chars: Vec<_> = "中文abc".chars().collect();
        let start = visible_start(&chars, chars.len(), 4, false);
        assert!(clean(&display_chars(&chars[start..], false)).width() < 4);
        assert_eq!(display_chars(&['a', '\t', 'b'], false), "a b");
    }
    #[tokio::test]
    async fn terminal_failure_does_not_cancel_the_running_operation() {
        let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let result = wait_with_ui(
            async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                completed.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(42)
            },
            || Err(anyhow::anyhow!("terminal disconnected")),
        )
        .await
        .unwrap();
        assert_eq!(result, 42);
        assert!(completed.load(std::sync::atomic::Ordering::SeqCst));
    }
}
