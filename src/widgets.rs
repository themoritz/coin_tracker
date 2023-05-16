use egui::{Pos2, Sense, TextStyle, Vec2, Widget, WidgetText};

pub struct BulletPoint {
    text: WidgetText,
}

impl BulletPoint {
    pub fn new(text: impl Into<WidgetText>) -> Self {
        BulletPoint { text: text.into() }
    }
}

impl Widget for BulletPoint {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let spacing = &ui.spacing();
        let extra = spacing.icon_width + spacing.icon_spacing;
        let wrap_width = ui.available_width() - extra;
        let text = self.text.into_galley(ui, None, wrap_width, TextStyle::Body);
        let desired_size = text.size() + Vec2::new(extra, 0.0);

        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        let dot = WidgetText::from("•").into_galley(ui, None, 5.0, TextStyle::Body);
        let dot_pos = Pos2::new(rect.min.x + 0.5 * extra - 0.5 * dot.size().x, rect.top());
        dot.paint_with_visuals(ui.painter(), dot_pos, ui.style().noninteractive());

        let text_pos = Pos2::new(rect.min.x + extra, rect.top());
        text.paint_with_visuals(ui.painter(), text_pos, ui.style().noninteractive());

        response
    }
}
