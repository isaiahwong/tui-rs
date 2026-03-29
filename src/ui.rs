use crossterm::{
    ExecutableCommand,
    event::{Event, EventStream, KeyCode, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt;
use ratatui::{Terminal, prelude::*};
use std::io::{self, stdout};
use tokio::sync::mpsc::Receiver;

use crate::{
    candle::CandleChart,
    types::{Candles, Message, Orderbook},
};

#[derive(Default)]
pub struct State {
    pub orderbook: Orderbook,
    pub candles: Candles,
    pub mouse_event: Option<crossterm::event::MouseEvent>,
}

pub struct App {
    rx: Receiver<Result<Message, anyhow::Error>>,
    quit: bool,
    state: State,
}

impl App {
    pub fn new(rx: Receiver<Result<Message, anyhow::Error>>) -> anyhow::Result<Self> {
        Ok(Self {
            rx,
            quit: false,
            state: State::default(),
        })
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
        enable_raw_mode()?;
        terminal.clear()?;
        stdout().execute(EnterAlternateScreen)?;
        stdout().execute(crossterm::event::EnableMouseCapture)?;

        let run_result = self.draw_loop(&mut terminal).await;

        stdout().execute(crossterm::event::DisableMouseCapture)?;
        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        run_result
    }

    async fn draw_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> anyhow::Result<()> {
        let mut events = EventStream::new();

        while !self.quit {
            tokio::select! {
                Some(msg) = self.rx.recv() => {
                    self.on_message(msg);
                }
                Some(Ok(event)) = events.next() => {
                    self.on_events(event);
                }
            }

            self.draw(terminal)?;
        }

        Ok(())
    }

    fn on_events(&mut self, event: Event) {
        match event {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    self.quit = true;
                }
            }
            Event::Mouse(mouse_event) => {
                self.state.mouse_event = Some(mouse_event);
            }
            _ => {}
        }
    }

    fn on_message(&mut self, msg: anyhow::Result<Message>) {
        let Ok(message) = msg else { return };
        match message {
            Message::BookSnapshot(depth) => {
                self.state.orderbook.apply_depth(depth);
            }
            Message::CandleSnapshot(klines) => {
                self.state.candles = klines.into();
            }
            Message::Candle(candle) => {
                if let Some(last) = self.state.candles.back_mut() {
                    if last.timestamp == candle.timestamp {
                        *last = candle;
                    } else {
                        self.state.candles.push_back(candle);
                        if self.state.candles.len() > 500 {
                            self.state.candles.pop_front();
                        }
                    }
                } else {
                    self.state.candles.push_back(candle);
                }
            }
        }
    }

    fn draw(&self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
        terminal.draw(|f| self.render(f))?;
        Ok(())
    }

    fn render(&self, f: &mut Frame) {
        let layout = Layout::default()
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.area());

        self.render_content(f, layout[1]);
    }

    fn render_content(&self, f: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        f.render_widget(
            CandleChart::new(&self.state.candles, self.state.mouse_event),
            cols[0],
        );
        f.render_widget(crate::dom::DomWidget::new(&self.state.orderbook), cols[1]);
    }
}

