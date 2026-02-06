use crate::factorio::{compact_number, signed_compact_number};

#[derive(Debug, Clone)]

pub struct SignedCompactLabel {
    pub value: f64,
    pub format: Option<String>,
}

pub struct CompactLabel {
    pub value: f64,
    pub format: Option<String>,
}

impl SignedCompactLabel {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            format: None,
        }
    }

    pub fn with_format(mut self, format: &str) -> Self {
        self.format = Some(format.to_string());
        self
    }
}

impl CompactLabel {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            format: None,
        }
    }

    pub fn with_format(mut self, format: &str) -> Self {
        self.format = Some(format.to_string());
        self
    }
}

impl egui::Widget for SignedCompactLabel {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let text = signed_compact_number(self.value);
        if let Some(format) = self.format {
            let formatted_text = format.replace("{}", &text);
            let label = ui.add(egui::Label::new(
                egui::RichText::new(&formatted_text)
                    .strong()
                    .size(ui.style().text_styles[&egui::TextStyle::Body].size * 0.9),
            ));
            let parsed_number = text.parse::<f64>();
            if parsed_number.is_err() {
                label.on_hover_text(self.value.to_string())
            } else if let Ok(n) = parsed_number
                && (f64::abs(n - self.value) > 1e-6 || self.value.abs() < 1e-5)
            {
                label.on_hover_text(self.value.to_string())
            } else {
                label
            }
        } else {
            let label =
                ui.add(egui::Label::new(egui::RichText::new(&text).strong().size(
                    ui.style().text_styles[&egui::TextStyle::Body].size * 0.9,
                )));
            let parsed_number = text.parse::<f64>();
            if parsed_number.is_err() {
                label.on_hover_text(self.value.to_string())
            } else if let Ok(n) = parsed_number
                && (f64::abs(n - self.value) > 1e-6 || self.value.abs() < 1e-5)
            {
                label.on_hover_text(self.value.to_string())
            } else {
                label
            }
        }
    }
}

impl egui::Widget for CompactLabel {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let text = compact_number(self.value);
        if let Some(format) = self.format {
            let formatted_text = format.replace("{}", &text);
            let label = ui.add(egui::Label::new(
                egui::RichText::new(&formatted_text)
                    .strong()
                    .size(ui.style().text_styles[&egui::TextStyle::Body].size * 0.9),
            ));
            let parsed_number = text.parse::<f64>();
            if parsed_number.is_err() {
                label.on_hover_text(self.value.to_string())
            } else if let Ok(n) = parsed_number
                && (f64::abs(n - self.value) > 1e-6 || self.value.abs() < 1e-5)
            {
                label.on_hover_text(self.value.to_string())
            } else {
                label
            }
        } else {
            let label =
                ui.add(egui::Label::new(egui::RichText::new(&text).strong().size(
                    ui.style().text_styles[&egui::TextStyle::Body].size * 0.9,
                )));
            let parsed_number = text.parse::<f64>();
            if parsed_number.is_err() {
                label.on_hover_text(self.value.to_string())
            } else if let Ok(n) = parsed_number
                && (f64::abs(n - self.value) > 1e-6 || self.value.abs() < 1e-5)
            {
                label.on_hover_text(self.value.to_string())
            } else {
                label
            }
        }
    }
}
