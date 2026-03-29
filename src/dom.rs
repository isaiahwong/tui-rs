use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph, Widget},
};

use crate::types::Orderbook;

pub struct DomWidget<'a> {
    ob: &'a Orderbook,
}

impl<'a> DomWidget<'a> {
    pub fn new(ob: &'a Orderbook) -> Self {
        Self { ob }
    }

    fn depth<'b>(
        price: &str,
        amount: &str,
        total: f64,
        max_cumulative: f64,
        area_width: u16,
        is_bid: bool,
    ) -> Line<'b> {
        let price_color = if is_bid { Color::Green } else { Color::Red };
        let bg_color = if is_bid {
            Color::Rgb(0, 50, 0)
        } else {
            Color::Rgb(50, 0, 0)
        };

        let text = format!("{:<10} {:>10} {:>10.4}", price, amount, total);
        let text_len = text.chars().count();

        let area_width_usize = area_width as usize;
        let padding = if area_width_usize > text_len {
            area_width_usize - text_len
        } else {
            0
        };

        let full_text = format!("{}{}", text, " ".repeat(padding));
        let full_text: String = full_text.chars().take(area_width_usize).collect();

        let bar_width = if max_cumulative > 0.0 {
            ((total / max_cumulative) * (area_width as f64)).round() as usize
        } else {
            0
        };
        let bar_width = bar_width.min(area_width_usize);

        let mut spans = Vec::new();
        let mut current_str = String::new();
        let mut current_style = Style::default();

        for (i, c) in full_text.chars().enumerate() {
            let fg = if i < 10 { price_color } else { Color::White };
            let bg = if i < bar_width {
                bg_color
            } else {
                Color::Reset
            };
            let style = Style::default().fg(fg).bg(bg);

            if current_str.is_empty() {
                current_str.push(c);
                current_style = style;
            } else if style == current_style {
                current_str.push(c);
            } else {
                spans.push(Span::styled(current_str, current_style));
                current_str = c.to_string();
                current_style = style;
            }
        }
        if !current_str.is_empty() {
            spans.push(Span::styled(current_str, current_style));
        }

        Line::from(spans)
    }

    fn render_side(area: Rect, ob: &Orderbook, is_bid: bool, buf: &mut ratatui::buffer::Buffer) {
        let limit = area.height as usize;

        let side = if is_bid {
            ob.bids(limit)
        } else {
            ob.asks(limit)
        };

        let max_cumulative = side.max_cumulative;

        let depths: Vec<ListItem> = side
            .depths
            .into_iter()
            .map(|(price, amount, cumulative)| {
                let p = format!("{:.4}", price);
                let a = format!("{:.4}", amount);
                let line = Self::depth(&p, &a, cumulative, max_cumulative, area.width, is_bid);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(depths);
        Widget::render(list, area, buf);
    }

    fn render_mid(area: Rect, ob: &Orderbook, buf: &mut ratatui::buffer::Buffer) {
        let mut mid_text = "Mid: --".to_string();

        if let (Some(mid), Some(spread)) = (ob.mid(), ob.spread()) {
            let spread_pct = (spread / mid) * 100.0;
            mid_text = format!(
                "Mid: {:.4} | Spread: {:.2} ({:.3}%)",
                mid, spread, spread_pct
            );
        }

        let p = Paragraph::new(mid_text)
            .alignment(ratatui::layout::Alignment::Center)
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        let vertical_centered = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(area);

        Widget::render(p, vertical_centered[1], buf);
    }
}

impl<'a> Widget for DomWidget<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let block = Block::bordered().border_style(Style::new().dark_gray());
        let block_area = block.inner(area);
        block.render(area, buf);

        let cols = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(45),
                Constraint::Percentage(5),
                Constraint::Percentage(45),
            ])
            .split(block_area);

        Self::render_side(cols[0], self.ob, false, buf);
        Self::render_mid(cols[1], self.ob, buf);
        Self::render_side(cols[2], self.ob, true, buf);
    }
}
