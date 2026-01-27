const LARGE_UNITS: [&str; 11] = ["", "k", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"];

pub fn signed_compact_number(num: f64) -> String {
    if num.is_sign_negative() {
        format!("-{}", compact_number(-num))
    } else {
        format!("+{}", compact_number(num))
    }
}

pub fn compact_number(num: f64) -> String {
    let abs_num = num.abs();

    match abs_num {
        n if n < 1e-9 => String::from("0"),
        n if n < 0.01 => format_with_unit(n * 1e6, "μ"),
        n => {
            let mut unit_idx = 0;
            let mut n = n;
            if n > 10000.0 {
                while n > 1000.0 && unit_idx < LARGE_UNITS.len() - 1 {
                    unit_idx += 1;
                    n /= 1000.0;
                }
            }
            format_with_unit(n, LARGE_UNITS[unit_idx])
        }
    }
}
fn format_with_unit(value: f64, unit: &str) -> String {
    let abs_value = value.abs();
    if unit.len() == 0 {
        if abs_value < 10.0 {
            // 对于小于10的值，最多保留3位小数
            let formatted = format!("{:.3}", value);
            format!(
                "{}{}",
                formatted.trim_end_matches('0').trim_end_matches('.'),
                unit
            )
        } else if abs_value < 100.0 {
            // 对于10-100的值，最多保留2位小数
            let formatted = format!("{:.2}", value);
            format!(
                "{}{}",
                formatted.trim_end_matches('0').trim_end_matches('.'),
                unit
            )
        } else if abs_value < 1000.0 {
            // 对于大于100的值，最多保留1位小数
            let formatted = format!("{:.1}", value);
            format!(
                "{}{}",
                formatted.trim_end_matches('0').trim_end_matches('.'),
                unit
            )
        } else {
            // 对于大于等于1000的值，取整
            format!("{}{}", value.round(), unit)
        }
    } else {
        if abs_value < 10.0 {
            // 对于小于10的值，最多保留2位小数
            let formatted = format!("{:.2}", value);
            format!(
                "{}{}",
                formatted.trim_end_matches('0').trim_end_matches('.'),
                unit
            )
        } else if abs_value < 100.0 {
            // 对于10-100的值，最多保留1位小数
            let formatted = format!("{:.1}", value);
            format!(
                "{}{}",
                formatted.trim_end_matches('0').trim_end_matches('.'),
                unit
            )
        } else {
            // 对于大于等于100的值，取整
            format!("{}{}", value.round(), unit)
        }
    }
}
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
        if self.format.is_some() {
            let fmt = self.format.unwrap();
            let formatted_text = fmt.replace("{}", &text);
            let label = ui.add(egui::Label::new(
                egui::RichText::new(&formatted_text)
                    .strong()
                    .size(ui.style().text_styles[&egui::TextStyle::Body].size * 0.9),
            ));
            let parsed_number = text.parse::<f64>();
            if parsed_number.is_err() {
                label.on_hover_text(self.value.to_string())
            } else if let Ok(n) = parsed_number
                && f64::abs(n - self.value) > 1e-6
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
                && f64::abs(n - self.value) > 1e-6
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
        if self.format.is_some() {
            let fmt = self.format.unwrap();
            let formatted_text = fmt.replace("{}", &text);
            let label = ui.add(egui::Label::new(
                egui::RichText::new(&formatted_text)
                    .strong()
                    .size(ui.style().text_styles[&egui::TextStyle::Body].size * 0.9),
            ));
            let parsed_number = text.parse::<f64>();
            if parsed_number.is_err() {
                label.on_hover_text(self.value.to_string())
            } else if let Ok(n) = parsed_number
                && f64::abs(n - self.value) > 1e-6
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
                && f64::abs(n - self.value) > 1e-6
            {
                label.on_hover_text(self.value.to_string())
            } else {
                label
            }
        }
    }
}

#[test]
fn test_compact_format() {
    dbg!(compact_number(1.1));
    dbg!(compact_number(114514.0));
    dbg!(compact_number(1919810.1));
    dbg!(compact_number(123456789.1));
    dbg!(compact_number(0.00011));
}
