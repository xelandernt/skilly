use super::*;

pub(crate) fn show_loading_message<T, F>(title: &str, message: &str, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    if !is_interactive_terminal() {
        return work();
    }
    let mut session = TerminalSession::new()?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });

    let mut frame_index = 0usize;
    loop {
        session.terminal.draw(|frame| {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(1),
                ])
                .split(frame.area());
            let spinner = LOADING_FRAMES[frame_index % LOADING_FRAMES.len()];
            frame.render_widget(
                Paragraph::new(title.to_string())
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                layout[0],
            );
            frame.render_widget(
                Paragraph::new(format!("{spinner} {message}"))
                    .block(Block::default().borders(Borders::ALL).title("Loading"))
                    .wrap(Wrap { trim: false }),
                layout[1],
            );
            frame.render_widget(
                Paragraph::new("Please wait...").style(
                    Style::default()
                        .fg(Color::Gray)
                        .add_modifier(Modifier::ITALIC),
                ),
                layout[2],
            );
        })?;

        match receiver.recv_timeout(Duration::from_millis(LOADING_POLL_INTERVAL_MS)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                frame_index = frame_index.wrapping_add(1);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("loading task ended unexpectedly");
            }
        }
    }
}
