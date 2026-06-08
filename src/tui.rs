use std::io::{self, stdout};

use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};

use crate::{
    message::{HarnessMessage, MessageLevel, MessageSource},
    state::{SharedAppState, SystemStatus},
};

/// Run the non-blocking TUI loop
/// The orchestration engine runs concurrently, communicating via SharedAppState and broadcast channel.
pub async fn run_tui(
    state: SharedAppState,
    mut log_rx: broadcast::Receiver<HarnessMessage>,
    prompt_tx: mpsc::UnboundedSender<String>,
    show_raw_json: bool,
) -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut logs: Vec<HarnessMessage> = Vec::new();
    let mut reader = EventStream::new();

    // Human-in-the-loop control states
    let mut selected_choice: usize = 0; // 0: Proceed, 1: Modify, 2: Abort
    let mut input_mode = false;
    let mut user_input = String::new();
    let mut startup_prompt = String::new();
    let mut awaiting_startup_prompt = true;
    let mut log_scroll: usize = 0;
    let mut log_page_height: usize = 1;
    let mut max_log_scroll: usize = 0;

    // Signal for shutdown
    let mut should_quit = false;

    while !should_quit {
        // Read current state for rendering
        let app_state = state.read().await.clone();

        terminal.draw(|f| {
            let area = f.area();
            let input_text = input_panel_text(
                awaiting_startup_prompt,
                &startup_prompt,
                app_state.status == SystemStatus::PausedForIntervention,
                input_mode,
                &user_input,
            );
            let input_height = input_panel_height(&input_text, area.width, area.height);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),            // Header / Status Bar
                    Constraint::Min(1),               // Log Stream (Central variable area)
                    Constraint::Length(input_height), // Input / Control Prompt
                ])
                .split(area);

            // 1. [Header / Status Bar]
            let status_color = match app_state.status {
                SystemStatus::Idle => Color::Gray,
                SystemStatus::Running => Color::Green,
                SystemStatus::ErrorRecovering => Color::Yellow,
                SystemStatus::PausedForIntervention => Color::Red, // Waiting for Approval
                SystemStatus::Halted => Color::DarkGray,
            };

            let active_tools = if app_state.active_tools.is_empty() {
                "None".to_owned()
            } else {
                let mut tools: Vec<_> = app_state.active_tools.values().map(|at| at.tool_id.clone()).collect();
                tools.sort();
                tools.join(", ")
            };
            let status_label = if awaiting_startup_prompt {
                "AwaitingPrompt".to_owned()
            } else {
                format!("{:?}", app_state.status)
            };
            let header_text = format!(
                " [Status: {}] | Active Tools: {} | Step: {} ",
                status_label,
                active_tools,
                app_state
                    .current_step
                    .clone()
                    .unwrap_or_else(|| "None".into())
            );

            let header = Paragraph::new(header_text)
                .style(
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Multi-Agent Orchestration Harness "),
                );
            f.render_widget(header, chunks[0]);

            // 2. [Log Stream]
            let display_logs: Vec<(String, MessageLevel)> = logs
                .iter()
                .filter_map(|msg| {
                    display_log_message(msg, show_raw_json)
                        .map(|content| (content, msg.level.clone()))
                })
                .collect();
            let log_width = chunks[1].width.saturating_sub(2).max(1) as usize;
            let log_height = chunks[1].height.saturating_sub(2) as usize;
            log_page_height = log_height.max(1);
            let log_view = visible_log_lines(&display_logs, log_width, log_height, log_scroll);
            max_log_scroll = log_view.max_scroll;
            log_scroll = log_scroll.min(max_log_scroll);

            let log_title = if max_log_scroll > 0 {
                format!(
                    " Event Logs & Agent Thoughts  Scroll {}/{} ",
                    log_scroll, max_log_scroll
                )
            } else {
                " Event Logs & Agent Thoughts ".to_owned()
            };
            let log_widget = Paragraph::new(log_view.lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("{log_title}  Up/Down PgUp/PgDn Home/End ")),
            );
            f.render_widget(log_widget, chunks[1]);

            // 3. [Input / Control Prompt]
            if awaiting_startup_prompt {
                let prompt = Paragraph::new(input_text)
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(Color::White))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Initial Prompt "),
                    );
                f.render_widget(prompt, chunks[2]);
            } else if app_state.status == SystemStatus::PausedForIntervention {
                if input_mode {
                    let prompt = Paragraph::new(input_text)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(Color::Yellow))
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Modify (Revise) Context "),
                        );
                    f.render_widget(prompt, chunks[2]);
                } else {
                    let options = ["[Proceed (진행)]", "[Modify (수정)]", "[Abort (중단)]"];
                    let mut line_spans = vec![];
                    for (i, opt) in options.iter().enumerate() {
                        let mut style = Style::default();
                        if i == selected_choice {
                            style = style.fg(Color::Black).bg(Color::White); // Selected highlight
                        }
                        line_spans.push(Span::styled(*opt, style));
                        line_spans.push(Span::raw("   "));
                    }
                    let prompt = Paragraph::new(Line::from(line_spans)).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Human-in-the-loop: Intervention Required "),
                    );
                    f.render_widget(prompt, chunks[2]);
                }
            } else {
                let prompt = Paragraph::new(input_text)
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(Color::DarkGray))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Control Prompt "),
                    );
                f.render_widget(prompt, chunks[2]);
            }
        })?;

        // Handle async events (tokio::select over TUI input vs Log streams)
        tokio::select! {
            // Receive new broadcast logs from core engine
            Ok(msg) = log_rx.recv() => {
                if is_prompt_ready_message(&msg) {
                    awaiting_startup_prompt = true;
                    startup_prompt.clear();
                }
                logs.push(msg);
            }
            // non-blocking event stream for keystrokes
            Some(Ok(evt)) = reader.next() => {
                if let Event::Key(key) = evt {
                    if !matches!(key.kind, KeyEventKind::Press) {
                        continue;
                    }

                    if matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
                        && key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        should_quit = true;
                        continue;
                    }

                    match key.code {
                        KeyCode::Up => {
                            log_scroll = (log_scroll + 1).min(max_log_scroll);
                            continue;
                        }
                        KeyCode::Down => {
                            log_scroll = log_scroll.saturating_sub(1);
                            continue;
                        }
                        KeyCode::PageUp => {
                            log_scroll =
                                log_scroll.saturating_add(log_page_height).min(max_log_scroll);
                            continue;
                        }
                        KeyCode::PageDown => {
                            log_scroll = log_scroll.saturating_sub(log_page_height);
                            continue;
                        }
                        KeyCode::Home => {
                            log_scroll = max_log_scroll;
                            continue;
                        }
                        KeyCode::End => {
                            log_scroll = 0;
                            continue;
                        }
                        _ => {}
                    }

                    if awaiting_startup_prompt {
                        match key.code {
                            KeyCode::Enter => {
                                let prompt = startup_prompt.trim().to_owned();
                                if is_quit_command(&prompt) {
                                    should_quit = true;
                                    continue;
                                }
                                if !prompt.is_empty() {
                                    let _ = prompt_tx.send(prompt);
                                    awaiting_startup_prompt = false;
                                    startup_prompt.clear();
                                }
                            }
                            KeyCode::Char(c) => {
                                startup_prompt.push(c);
                            }
                            KeyCode::Backspace => {
                                startup_prompt.pop();
                            }
                            _ => {}
                        }
                        continue;
                    }

                    let mut st = state.write().await;
                    if st.status == SystemStatus::PausedForIntervention {
                        if input_mode {
                            match key.code {
                                KeyCode::Enter => {
                                    // Modify operation selected & submitted
                                    st.human_intervention_feedback = Some(user_input.clone());

                                    // Resume the core loop (clear intervention lock)
                                    st.intervention_required = None;
                                    st.status = SystemStatus::Running;
                                    input_mode = false;
                                    user_input.clear();
                                }
                                KeyCode::Esc => {
                                    input_mode = false;
                                    user_input.clear();
                                }
                                KeyCode::Char(c) => {
                                    user_input.push(c);
                                }
                                KeyCode::Backspace => {
                                    user_input.pop();
                                }
                                _ => {}
                            }
                        } else {
                            match key.code {
                                KeyCode::Left => {
                                    if selected_choice > 0 {
                                        selected_choice -= 1;
                                    }
                                }
                                KeyCode::Right => {
                                    if selected_choice < 2 {
                                        selected_choice += 1;
                                    }
                                }
                                KeyCode::Enter => {
                                    match selected_choice {
                                        0 => {
                                            // Proceed (진행)
                                            st.intervention_required = None;
                                            st.status = SystemStatus::Running;
                                        }
                                        1 => {
                                            // Modify (수정) -> Switch to text feedback mode
                                            input_mode = true;
                                        }
                                        2 => {
                                            // Abort (중단)
                                            st.status = SystemStatus::Halted;
                                            st.intervention_required = None;
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else if st.status == SystemStatus::Running || st.status == SystemStatus::ErrorRecovering {
                        if matches!(key.code, KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Esc) {
                            st.status = SystemStatus::Halted;
                            st.intervention_required = None;
                        }
                    }
                }
            }
        }
    }

    // Graceful cleanup routine when Loop breaks
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;

    // Update global state to HALTED so running CLI tokens/spawns wrap up or get killed
    {
        let mut st = state.write().await;
        st.status = SystemStatus::Halted;
    }

    Ok(())
}

struct LogView {
    lines: Vec<Line<'static>>,
    max_scroll: usize,
}

fn visible_log_lines(
    display_logs: &[(String, MessageLevel)],
    width: usize,
    height: usize,
    scroll_from_bottom: usize,
) -> LogView {
    let mut all_lines = Vec::new();

    for (content, level) in display_logs {
        let style = Style::default().fg(level_color(level));
        for line in content.lines() {
            for segment in wrap_line(line, width) {
                all_lines.push(Line::from(Span::styled(segment, style)));
            }
        }
    }

    let height = height.max(1);
    let max_scroll = all_lines.len().saturating_sub(height);
    let scroll = scroll_from_bottom.min(max_scroll);
    let end = all_lines.len().saturating_sub(scroll);
    let start = end.saturating_sub(height);

    LogView {
        lines: all_lines[start..end].to_vec(),
        max_scroll,
    }
}

fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    if line.is_empty() {
        return vec![String::new()];
    }

    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for character in line.chars() {
        let character_width = display_width(character);
        if current_width > 0 && current_width + character_width > width {
            segments.push(current);
            current = String::new();
            current_width = 0;
        }

        current.push(character);
        current_width += character_width;
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

fn input_panel_text(
    awaiting_prompt: bool,
    prompt: &str,
    paused_for_intervention: bool,
    input_mode: bool,
    feedback: &str,
) -> String {
    if awaiting_prompt {
        format!("Prompt: {prompt}_")
    } else if paused_for_intervention && input_mode {
        format!("Feedback: {feedback}_")
    } else if paused_for_intervention {
        "[Proceed (진행)]   [Modify (수정)]   [Abort (중단)]".to_owned()
    } else {
        "작업 진행 중... (중단: Esc 또는 A, 종료: Ctrl+C)".to_owned()
    }
}

fn is_quit_command(input: &str) -> bool {
    matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "/quit" | "/exit"
    )
}

fn input_panel_height(text: &str, terminal_width: u16, terminal_height: u16) -> u16 {
    let content_width = terminal_width.saturating_sub(2).max(1) as usize;
    let wrapped_lines = wrapped_line_count(text, content_width).max(1) as u16;
    let desired = wrapped_lines.saturating_add(2).max(3);
    let max_height = terminal_height.saturating_sub(6).max(3);
    desired.min(max_height)
}

fn display_log_message(message: &HarnessMessage, show_raw_json: bool) -> Option<String> {
    let display = if show_raw_json {
        Some(message.raw_output.clone())
    } else {
        display_output(message)
    }?;

    if display.trim().is_empty() {
        return None;
    }

    let kst_offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    let time = message
        .timestamp
        .with_timezone(&kst_offset)
        .format("%H:%M:%S")
        .to_string();

    Some(format!(
        "[{}] [{:?}] [{}] {}",
        time, message.level, message.tool_id, display
    ))
}

fn is_json_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    if serde_json::from_str::<Value>(trimmed).is_ok() {
        return true;
    }

    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        return true;
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start < end {
            let sub = &trimmed[start..=end];
            if serde_json::from_str::<Value>(sub).is_ok() {
                return true;
            }
        }
    }

    if let (Some(start), Some(end)) = (trimmed.find('['), trimmed.rfind(']')) {
        if start < end {
            let sub = &trimmed[start..=end];
            if serde_json::from_str::<Value>(sub).is_ok() {
                return true;
            }
        }
    }

    false
}

fn filter_json_lines(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !is_json_line(line))
        .collect();
    lines.join("\n")
}

fn display_output(message: &HarnessMessage) -> Option<String> {
    if message.source == MessageSource::System {
        return Some(message.raw_output.clone());
    }

    let raw_result = if let Some(text) = extract_response_from_text(&message.raw_output) {
        Some(text)
    } else if let Some(text) = extract_response_from_value(&message.normalized_payload) {
        Some(text)
    } else if looks_like_json(&message.raw_output) {
        None
    } else {
        Some(message.raw_output.clone())
    };

    raw_result
        .map(|text| filter_json_lines(&text))
        .filter(|text| !text.trim().is_empty())
}

fn level_color(level: &MessageLevel) -> Color {
    match level {
        MessageLevel::Error => Color::Red,
        MessageLevel::Warn => Color::Yellow,
        MessageLevel::Info => Color::White,
        MessageLevel::Debug => Color::DarkGray,
    }
}

fn wrapped_line_count(text: &str, width: usize) -> usize {
    let width = width.max(1);
    text.lines()
        .map(|line| line_display_width(line).max(1).div_ceil(width))
        .sum::<usize>()
        .max(1)
}

fn line_display_width(line: &str) -> usize {
    line.chars().map(display_width).sum()
}

fn display_width(character: char) -> usize {
    if character.is_control() {
        return 0;
    }

    if is_wide_character(character) {
        2
    } else {
        1
    }
}

fn is_wide_character(character: char) -> bool {
    matches!(
        character as u32,
        0x1100..=0x115F
            | 0x2329..=0x232A
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
            | 0x20000..=0x3FFFD
    )
}

fn is_prompt_ready_message(message: &HarnessMessage) -> bool {
    message
        .normalized_payload
        .get("event")
        .and_then(Value::as_str)
        == Some("awaiting_prompt")
}

fn looks_like_json(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn extract_response_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return extract_response_from_value(&value);
    }

    let parts: Vec<String> = trimmed
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .filter_map(|value| extract_response_from_value(&value))
        .filter(|part| !part.trim().is_empty())
        .collect();

    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn extract_response_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => non_empty(text),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(extract_response_from_value)
                .filter(|part| !part.trim().is_empty())
                .collect();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        Value::Object(map) => {
            for key in [
                "response",
                "answer",
                "message",
                "text",
                "content",
                "output",
                "last_agent_message",
                "final_answer",
            ] {
                if let Some(text) = map.get(key).and_then(extract_response_from_value) {
                    return Some(text);
                }
            }

            for key in ["stdout_text", "stderr_text", "line"] {
                if let Some(text) = map.get(key).and_then(Value::as_str) {
                    if let Some(response) = extract_response_from_text(text) {
                        return Some(response);
                    }
                }
            }

            for key in ["msg", "data", "delta", "result", "event", "item", "items"] {
                if let Some(text) = map.get(key).and_then(extract_response_from_value) {
                    return Some(text);
                }
            }

            None
        }
        _ => None,
    }
}

fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}
