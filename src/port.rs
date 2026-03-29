impl App {
    fn render_header(&self, f: &mut ratatui::Frame, area: Rect) {
        let price = self.candles.last_price();
        let pct = self.candles.pct_change();

        let title = Span::styled(
            self.symbol.to_uppercase(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

        let pct_color = {
            if pct > 0.0 {
                TMUX_GREEN
            } else if pct < 0.0 {
                Color::Red
            } else {
                Color::White
            }
        };

        let line = Line::from(vec![
            Span::raw(" Last: "),
            Span::styled(format!("{price:.4}"), Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::styled(format!("{pct:+.2}%"), Style::default().fg(pct_color)),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(title);

        f.render_widget(Paragraph::new(line).block(block), area);
    }
}

pub struct CrossHair {
    candle: Candle,
    col: u16,
    row: u16,
}

impl CrossHair {
    pub fn new<'a>(
        mut candles: impl Iterator<Item = &'a Candle>,
        area: Rect,
        mouse: MouseEvent,
    ) -> Option<Self> {
        let max_x = area.x.saturating_add(area.width);
        let max_y = area.y.saturating_add(area.height);
        let bounds_x = area.x..max_x;
        let bounds_y = area.y..max_y;

        let is_inside = bounds_x.contains(&mouse.column) && bounds_y.contains(&mouse.row);
        if !is_inside {
            return None;
        }

        let col = mouse.column;
        let row = mouse.row;
        let candle = candles.nth(usize::from(col - area.x)).copied()?;

        Some(Self { candle, col, row })
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let guide_style = Style::default().bg(Color::DarkGray);
        let center_style = Style::default().bg(Color::Gray);

        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, self.row)) {
                cell.set_style(guide_style);
            }
        }

        for y in area.y..area.y + area.height {
            if let Some(cell) = buf.cell_mut((self.col, y)) {
                cell.set_style(guide_style);
            }
        }

        if let Some(cell) = buf.cell_mut((self.col, self.row)) {
            cell.set_style(center_style);
        }
    }

    pub fn render_price(
        &self,
        plot_area: Rect,
        axis_area: Rect,
        scale: &PriceScale,
        buf: &mut Buffer,
    ) {
        if axis_area.width == 0 || axis_area.height == 0 {
            return;
        }

        let local_row = self
            .row
            .saturating_sub(plot_area.y)
            .min(plot_area.height.saturating_sub(1));
        let price = scale.to_price(local_row);
        let width = axis_area.width as usize;
        let label = format!("{:>width$.2}", price, width = width);
        let style = Style::default()
            .fg(Color::Black)
            .bg(Color::Gray)
            .add_modifier(Modifier::BOLD);

        buf.set_stringn(axis_area.x, self.row, label, width, style);
    }
}

pub struct DomWidget<'a> {
    book: &'a Book,
}

impl<'a> DomWidget<'a> {
    pub fn new(book: &'a Book) -> Self {
        Self { book }
    }

    fn accumulate(levels: &[BookLevel]) -> Vec<(BookLevel, f64)> {
        let mut total = 0.0;
        levels
            .iter()
            .copied()
            .map(|level| {
                total += level.qty;
                (level, total)
            })
            .collect()
    }

    fn depth_bg(is_bid: bool, accum: f64, max_accum: f64) -> Color {
        let ratio = if max_accum <= f64::EPSILON {
            0.0
        } else {
            (accum / max_accum).clamp(0.0, 1.0)
        };
        let intensity = (24.0 + ratio * 64.0).round() as u8;

        if is_bid {
            Color::Rgb(0, intensity, intensity / 3)
        } else {
            Color::Rgb(intensity, 0, 0)
        }
    }

    fn depth_width(accum: f64, max_accum: f64, width: u16) -> u16 {
        if max_accum <= f64::EPSILON || width == 0 {
            return 0;
        }

        ((accum / max_accum).clamp(0.0, 1.0) * f64::from(width)).round() as u16
    }

    fn render_level(
        area: Rect,
        y: u16,
        level: BookLevel,
        accum: f64,
        max_accum: f64,
        is_bid: bool,
        buf: &mut Buffer,
    ) {
        let color = if is_bid { TMUX_GREEN } else { Color::Red };
        let bg = Self::depth_bg(is_bid, accum, max_accum);
        let depth_width = Self::depth_width(accum, max_accum, area.width);

        for x in area.x..area.x + depth_width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(bg);
            }
        }

        let price = format!("{:>10.2}", level.price);
        let qty = format!("{:>8.4}", level.qty);
        buf.set_stringn(
            area.x,
            y,
            price,
            area.width as usize,
            Style::default().fg(color),
        );

        if area.width > 11 {
            buf.set_stringn(
                area.x + 11,
                y,
                qty,
                area.width.saturating_sub(11) as usize,
                Style::default().fg(Color::White),
            );
        }
    }

    fn spread_label(&self) -> String {
        self.book
            .spread()
            .map(|spread| format!("Spread {:>8.2}", spread))
            .unwrap_or_else(|| "Spread       --".to_string())
    }

    fn render_rows(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let body_rows = area.height.saturating_sub(1);
        let ask_slots = usize::from(body_rows / 2);
        let bid_slots = usize::from(body_rows - body_rows / 2);

        let asks = Self::accumulate(&self.book.asks()[..self.book.asks().len().min(ask_slots)]);
        let bids = Self::accumulate(&self.book.bids()[..self.book.bids().len().min(bid_slots)]);
        let ask_max = asks.last().map(|(_, accum)| *accum).unwrap_or(0.0);
        let bid_max = bids.last().map(|(_, accum)| *accum).unwrap_or(0.0);
        let ask_y = area.y + ask_slots.saturating_sub(asks.len()) as u16;
        let spread_y = area.y + ask_slots as u16;

        for (row, (level, accum)) in asks.into_iter().rev().enumerate() {
            Self::render_level(area, ask_y + row as u16, level, accum, ask_max, false, buf);
        }

        buf.set_stringn(
            area.x,
            spread_y,
            self.spread_label(),
            area.width as usize,
            Style::default().fg(Color::Gray),
        );

        for (row, (level, accum)) in bids.into_iter().enumerate() {
            Self::render_level(
                area,
                spread_y + 1 + row as u16,
                level,
                accum,
                bid_max,
                true,
                buf,
            );
        }
    }
}

impl Widget for DomWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("Orderbook")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        block.render(area, buf);

        if self.book.is_empty() {
            buf.set_stringn(
                inner.x,
                inner.y,
                "waiting for book",
                inner.width as usize,
                Style::default().fg(Color::Gray),
            );
            return;
        }

        self.render_rows(inner, buf);
    }
}

pub struct CandleStickChart<'a> {
    candles: &'a Candles,
    mouse_event: Option<MouseEvent>,
}

impl<'a> CandleStickChart<'a> {
    pub fn new(candles: &'a Candles, mouse_event: Option<MouseEvent>) -> Self {
        Self {
            candles,
            mouse_event,
        }
    }

    fn render_header(candle: Option<Candle>, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let Some(candle) = candle else {
            return;
        };

        let color = if candle.is_bullish() {
            TMUX_GREEN
        } else {
            Color::Red
        };

        let style = Style::default().fg(color);

        let line = Line::from(vec![
            Span::styled(format!("O: {:.4} ", candle.open), style),
            Span::styled(format!("H: {:.4} ", candle.high), style),
            Span::styled(format!("L: {:.4} ", candle.low), style),
            Span::styled(format!("C: {:.4} ", candle.close), style),
            Span::styled(format!("V: {:.2}", candle.volume), style),
        ]);

        Paragraph::new(line).render(area, buf);
    }

    fn render_axis(scale: &PriceScale, area: &Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let text_style = Style::default().fg(Color::Gray);
        let tick_count = area.height.min(6);
        if tick_count < 2 {
            return;
        }
        for i in 0..tick_count {
            let row = i * (area.height - 1) / (tick_count - 1);
            let price = scale.to_price(row);
            let label = format!("{price:>8.2}");
            buf.set_stringn(area.x, area.y + row, label, area.width as usize, text_style);
        }
    }
}

impl Widget for CandleStickChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let block_area = block.inner(area);
        block.render(area, buf);

        if self.candles.len() == 0 || block_area.width == 0 || block_area.height == 0 {
            return;
        }

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(block_area);
        let header_area = sections[0];
        let content_area = sections[1];

        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        let sections = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(PRICE_AXIS_WIDTH)])
            .split(content_area);
        let plot_area = sections[0];
        let axis_area = sections[1];

        if plot_area.width == 0 {
            return;
        }

        let skip = self.candles.len().saturating_sub(plot_area.width as usize);
        let visible = || self.candles.iter().skip(skip);

        let scale = PriceScale::new(visible(), plot_area.height);
        let crosshair = self
            .mouse_event
            .and_then(|mouse| CrossHair::new(visible(), plot_area, mouse));

        let header_candle = crosshair
            .as_ref()
            .map(|crosshair| crosshair.candle)
            .or_else(|| visible().last().copied());

        Self::render_header(header_candle, header_area, buf);

        for (col, candle) in visible().enumerate() {
            CandleStick::render(candle, plot_area.x + col as u16, &scale, plot_area, buf);
        }

        Self::render_axis(&scale, &axis_area, buf);

        if let Some(crosshair) = crosshair {
            crosshair.render(plot_area, buf);
            crosshair.render_price(plot_area, axis_area, &scale, buf);
        }
    }
}

