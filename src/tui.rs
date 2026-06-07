use std::io::{self, stdout};

use crossterm::{
    event::{Event, EventStream, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use tokio::fs;
use tokio::sync::{broadcast, oneshot};

use crate::{
    message::{HarnessMessage, MessageLevel},
    orchestration::CONTEXT_FILE_NAME,
    state::{SharedAppState, SystemStatus},
};

/// Run the non-blocking TUI loop
/// The orchestration engine runs concurrently, communicating via SharedAppState and broadcast channel.
pub async fn run_tui(
    state: SharedAppState,
    mut log_rx: broadcast::Receiver<HarnessMessage>,
    mut initial_prompt_tx: Option<oneshot::Sender<String>>,
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
    let mut awaiting_startup_prompt = initial_prompt_tx.is_some();

    // Signal for shutdown
    let mut should_quit = false;

    while !should_quit {
        // Read current state for rendering
        let app_state = state.read().await.clone();

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header / Status Bar
                    Constraint::Min(1),    // Log Stream (Central variable area)
                    Constraint::Length(3), // Input / Control Prompt
                ])
                .split(f.area());

            // 1. [Header / Status Bar]
            let status_color = match app_state.status {
                SystemStatus::Idle => Color::Gray,
                SystemStatus::Running => Color::Green,
                SystemStatus::ErrorRecovering => Color::Yellow,
                SystemStatus::PausedForIntervention => Color::Red, // Waiting for Approval
                SystemStatus::Halted => Color::DarkGray,
            };

            let current_tool = app_state
                .current_tool
                .clone()
                .unwrap_or_else(|| "None".into());
            let status_label = if awaiting_startup_prompt {
                "AwaitingPrompt".to_owned()
            } else {
                format!("{:?}", app_state.status)
            };
            let header_text = format!(
                " [Status: {}] | Active Tool: {} | Step: {} ",
                status_label,
                current_tool,
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
            let log_items: Vec<ListItem> = logs
                .iter()
                .map(|msg| {
                    let kst_offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
                    let time = msg
                        .timestamp
                        .with_timezone(&kst_offset)
                        .format("%H:%M:%S")
                        .to_string();
                    let content = format!("[{}] [{:?}] {}", time, msg.level, msg.raw_output);

                    let color = match msg.level {
                        MessageLevel::Error => Color::Red,
                        MessageLevel::Warn => Color::Yellow,
                        MessageLevel::Info => Color::White,
                        MessageLevel::Debug => Color::DarkGray,
                    };

                    ListItem::new(Line::from(Span::styled(
                        content,
                        Style::default().fg(color),
                    )))
                })
                .collect();

            let log_widget = List::new(log_items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Event Logs & Agent Thoughts "),
            );

            // Auto-scroll to bottom behavior
            let mut list_state = ListState::default();
            if !logs.is_empty() {
                list_state.select(Some(logs.len() - 1));
            }
            f.render_stateful_widget(log_widget, chunks[1], &mut list_state);

            // 3. [Input / Control Prompt]
            if awaiting_startup_prompt {
                let prompt = Paragraph::new(format!("Prompt: {}_", startup_prompt))
                    .style(Style::default().fg(Color::White))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Initial Prompt "),
                    );
                f.render_widget(prompt, chunks[2]);
            } else if app_state.status == SystemStatus::PausedForIntervention {
                if input_mode {
                    let prompt = Paragraph::new(format!("Feedback: {}_", user_input))
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
                let prompt = Paragraph::new("작업 진행 중... (종료: q 또는 Ctrl+C)")
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
                logs.push(msg);
            }
            // non-blocking event stream for keystrokes
            Some(Ok(evt)) = reader.next() => {
                if let Event::Key(key) = evt {
                    // Global shutdown intercept (Graceful shutdown request)
                    if key.code == KeyCode::Char('q')
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL))
                    {
                        should_quit = true;
                        continue;
                    }

                    if awaiting_startup_prompt {
                        match key.code {
                            KeyCode::Enter => {
                                let prompt = startup_prompt.trim().to_owned();
                                if !prompt.is_empty() {
                                    if let Some(sender) = initial_prompt_tx.take() {
                                        let _ = sender.send(prompt);
                                    }
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
                                    // Inject feedback to `.harness_context.json`
                                    let path = std::path::Path::new(CONTEXT_FILE_NAME);
                                    if let Ok(content) = fs::read_to_string(path).await {
                                        if let Ok(mut json_val) = serde_json::from_str::<serde_json::Value>(&content) {
                                            json_val["human_intervention_feedback"] = serde_json::Value::String(user_input.clone());
                                            let _ = fs::write(path, serde_json::to_string_pretty(&json_val).unwrap()).await;
                                        }
                                    }

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
                                            should_quit = true;
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
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
