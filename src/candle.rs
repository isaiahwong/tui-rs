use std::ops::RangeInclusive;

use crossterm::event::MouseEvent;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

use crate::types::{Candle, Candles};

const GREEN: Color = Color::Rgb(94, 186, 137);

pub struct CandleChart<'a> {
    candles: &'a Candles,
    mouse_event: Option<MouseEvent>,
}

impl<'a> CandleChart<'a> {
    const PRICE_AXIS_WIDTH: u16 = 10;

    pub fn new(candles: &'a Candles, mouse_event: Option<MouseEvent>) -> Self {
        Self {
            candles,
            mouse_event,
        }
    }

    fn render_chart(
        candles: impl Iterator<Item = &'a Candle>,
        scale: &PriceScale,
        area: Rect,
        buf: &mut Buffer,
    ) {
        for (col, candle) in candles.enumerate() {
            let candle_area = Rect {
                x: area.x + col as u16,
                width: 1,
                ..area
            };
            CandleStick::new(candle, scale).render(candle_area, buf);
        }
    }

    fn render_header(candle: &Candle, area: Rect, buf: &mut Buffer) {
        let color = if candle.is_bullish() { GREEN } else { Color::Red };

        let line = Line::from(vec![
            format!("O: {:.4} ", candle.open).fg(color),
            format!("H: {:.4} ", candle.high).fg(color),
            format!("L: {:.4} ", candle.low).fg(color),
            format!("C: {:.4} ", candle.close).fg(color),
            format!("V: {:.2}", candle.volume).fg(color),
        ]);

        Paragraph::new(line).render(area, buf);
    }

    fn render_price(
        crosshair: &Crosshair,
        scale: &PriceScale,
        plot_area: Rect,
        axis_area: Rect,
        buf: &mut Buffer,
    ) {
        let local_row = crosshair
            .row
            .saturating_sub(plot_area.y)
            .min(plot_area.height.saturating_sub(1));

        let price = scale.to_price(local_row, 1);
        let width = axis_area.width as usize;
        let label = format!("{price:>width$.4}");
        
        let style = Style::new().black().on_gray().bold();

        buf.set_stringn(axis_area.x, crosshair.row, label, width, style);
    }

    fn render_axis(scale: &PriceScale, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let text_style = Style::new().dark_gray();
        let tick_count = area.height.min(6);
        for i in 0..tick_count {
            let row = i * (area.height - 1) / (tick_count - 1);
            let price = scale.to_price(row * CandleStick::PIXEL_MUL, CandleStick::PIXEL_MUL);
            let label = format!("{price:>8.4}");
            buf.set_stringn(area.x, area.y + row, label, area.width as usize, text_style);
        }
    }
}

impl<'a> Widget for CandleChart<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered().border_style(Style::new().dark_gray());
        let block_area = block.inner(area);
        block.render(area, buf);

        let [header_area, chart_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(block_area);

        let [plot_area, axis_area] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(Self::PRICE_AXIS_WIDTH),
        ])
        .areas(chart_area);

        let skip = self.candles.len().saturating_sub(plot_area.width as usize);
        let visible_candles = || self.candles.iter().skip(skip);

        let scale = PriceScale::new(visible_candles(), plot_area.height);

        let crosshair = self
            .mouse_event
            .and_then(|mouse| Crosshair::new(visible_candles(), plot_area, mouse));

        let header_candle = crosshair
            .as_ref()
            .map(|crosshair| crosshair.candle)
            .or_else(|| visible_candles().last());

        Self::render_chart(visible_candles(), &scale, plot_area, buf);
        Self::render_axis(&scale, axis_area, buf);

        if let Some(crosshair) = crosshair {
            Self::render_price(&crosshair, &scale, plot_area, axis_area, buf);
            crosshair.render(plot_area, buf);
        }

        if let Some(candle) = header_candle {
            Self::render_header(candle, header_area, buf);
        }
    }
}

pub enum Cell {
    Empty,
    Wick,
    Body,
    Doji,
}

impl Cell {
    pub fn new(
        row: u16,
        body: &RangeInclusive<u16>,
        wick: &RangeInclusive<u16>,
        is_doji: bool,
    ) -> Self {
        if body.contains(&row) {
            if is_doji { Self::Doji } else { Self::Body }
        } else if wick.contains(&row) {
            Self::Wick
        } else {
            Self::Empty
        }
    }
}

pub struct CandleStick<'a> {
    candle: &'a Candle,
    scale: &'a PriceScale,
}

impl<'a> CandleStick<'a> {
    const PIXEL_MUL: u16 = 2;

    pub fn new(candle: &'a Candle, scale: &'a PriceScale) -> Self {
        Self { candle, scale }
    }

    fn glyph(row: u16, candle: &Candle, scale: &PriceScale) -> Option<&'static str> {
        let to_row = |price: f64| scale.to_row(price, Self::PIXEL_MUL);

        let open = to_row(candle.open);
        let close = to_row(candle.close);
        let high = to_row(candle.high);
        let low = to_row(candle.low);

        let body_top = open.min(close);
        let body_bot = open.max(close);
        let is_doji = body_top == body_bot;

        let body_range = body_top..=body_bot;
        let wick_range = high..=low;
        let has_upper_wick = high < body_top;
        let has_lower_wick = low > body_bot;

        let upper_row = row * Self::PIXEL_MUL;
        let lower_row = upper_row + 1;

        let upper_cell = Cell::new(upper_row, &body_range, &wick_range, is_doji);
        let lower_cell = Cell::new(lower_row, &body_range, &wick_range, is_doji);

        match (&upper_cell, &lower_cell) {
            (Cell::Body, Cell::Body | Cell::Wick) => Some("█"),
            (Cell::Body, Cell::Empty | Cell::Doji) => Some("▀"),
            (Cell::Doji, Cell::Doji) => Some("━"),
            (Cell::Doji, Cell::Body) => Some("▄"),
            (Cell::Doji, Cell::Wick) => Some(if has_upper_wick { "┿" } else { "┯" }),
            (Cell::Doji, Cell::Empty) => Some(if has_upper_wick { "┷" } else { "━" }),
            (Cell::Wick, Cell::Wick) => Some("│"),
            (Cell::Wick, Cell::Body) => Some("█"),
            (Cell::Wick, Cell::Empty) => Some("╵"),
            (Cell::Wick, Cell::Doji) => Some(if has_lower_wick { "┿" } else { "┷" }),
            (Cell::Empty, Cell::Doji) => Some(if has_lower_wick { "┯" } else { "━" }),
            (Cell::Empty, Cell::Body) => Some("▄"),
            (Cell::Empty, Cell::Wick) => Some("╷"),
            (Cell::Empty, Cell::Empty) => None,
        }
    }
}

impl<'a> Widget for CandleStick<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_bullish = self.candle.is_bullish();
        let color = if is_bullish { GREEN } else { Color::Red };
        let style = Style::new().fg(color);

        for row in 0..area.height {
            let Some(glyph) = Self::glyph(row, self.candle, self.scale) else {
                continue;
            };

            if let Some(cell) = buf.cell_mut((area.x, area.y + row)) {
                cell.set_symbol(glyph).set_style(style);
            }
        }
    }
}

struct Crosshair<'a> {
    candle: &'a Candle,
    col: u16,
    row: u16,
}

impl<'a> Crosshair<'a> {
    pub fn new(
        mut candles: impl Iterator<Item = &'a Candle>,
        area: Rect,
        mouse: MouseEvent,
    ) -> Option<Self> {
        if !area.contains((mouse.column, mouse.row).into()) {
            return None;
        }

        let col = mouse.column;
        let row = mouse.row;
        let candle = candles.nth(usize::from(col - area.x))?;

        Some(Self { candle, col, row })
    }
}

impl<'a> Widget for Crosshair<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Render column
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, self.row)) {
                cell.set_style(cell.style().bg(Color::DarkGray));
            }
        }

        // Render row
        for y in area.top()..area.bottom() {
            if let Some(cell) = buf.cell_mut((self.col, y)) {
                cell.set_style(cell.style().bg(Color::DarkGray));
            }
        }

        if let Some(cell) = buf.cell_mut((self.col, self.row)) {
            cell.set_style(cell.style().bg(Color::Gray));
        }
    }
}

pub struct PriceScale {
    price_max: f64,
    price_range: f64,
    area_height: u16,
}

impl PriceScale {
    pub fn new<'a>(candles: impl Iterator<Item = &'a Candle>, area_height: u16) -> Self {
        let (min, max) = candles.fold((f64::MAX, f64::MIN), |(lo, hi), c| {
            (lo.min(c.low), hi.max(c.high))
        });

        Self {
            price_max: max,
            price_range: max - min,
            area_height,
        }
    }

    pub fn to_row(&self, price: f64, pixel_mul: u16) -> u16 {
        let height = (self.area_height * pixel_mul).saturating_sub(1) as f64;
        let pct = (self.price_max - price) / self.price_range;
        (pct * height).round().clamp(0.0, height) as u16
    }

    pub fn to_price(&self, row: u16, pixel_mul: u16) -> f64 {
        let height = (self.area_height * pixel_mul).saturating_sub(1) as f64;
        let pct = f64::from(row) / height;
        self.price_max - pct * self.price_range
    }
}
