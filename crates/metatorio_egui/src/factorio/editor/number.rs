use crate::factorio::{TimeScale, compact_number, signed_compact_number};

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
                    .size(ui.style().text_styles[&egui::TextStyle::Body].size * 0.875),
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
                    ui.style().text_styles[&egui::TextStyle::Body].size * 0.875,
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
                    .size(ui.style().text_styles[&egui::TextStyle::Body].size * 0.875),
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
                    ui.style().text_styles[&egui::TextStyle::Body].size * 0.875,
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

pub struct AmountLabel {
    amount: f64,
    time_scale: TimeScale,
    is_energy: bool,
    is_signed: bool,
}

impl AmountLabel {
    pub fn new(amount: f64) -> Self {
        Self {
            amount,
            time_scale: TimeScale::Seconds,
            is_energy: false,
            is_signed: false,
        }
    }

    pub fn with_time_scale(mut self, time_scale: TimeScale) -> Self {
        self.time_scale = time_scale;
        self
    }
    pub fn with_is_energy(mut self, is_energy: bool) -> Self {
        self.is_energy = is_energy;
        self
    }
    pub fn with_is_signed(mut self, is_signed: bool) -> Self {
        self.is_signed = is_signed;
        self
    }
}

impl egui::Widget for AmountLabel {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let scaled_amount = if self.is_energy {
            self.amount
        } else {
            match self.time_scale {
                TimeScale::Seconds => self.amount,
                TimeScale::Minutes => self.amount * 60.0,
                TimeScale::Hours => self.amount * 3600.0,
            }
        };
        if self.is_signed {
            let label = if self.is_energy {
                SignedCompactLabel::new(scaled_amount).with_format("{}W")
            } else {
                SignedCompactLabel::new(scaled_amount)
            };
            label.ui(ui)
        } else {
            let label = if self.is_energy {
                CompactLabel::new(scaled_amount).with_format("{}W")
            } else {
                CompactLabel::new(scaled_amount)
            };
            label.ui(ui)
        }
    }
}
