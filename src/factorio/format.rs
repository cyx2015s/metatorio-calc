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
        n if n < 1.0 => {
            // 处理小于1的数，最多保留2位小数
            if n >= 0.01 {
                format!("{:.2}", num)
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string()
            } else if n >= 0.001 {
                format!("{:.3}", num).to_string()
            } else {
                "0".to_string()
            }
        }
        n if n < 10.0 => {
            // 1-10：最多保留2位小数
            if n.fract() == 0.0 {
                format!("{:.0}", num)
            } else {
                let formatted = format!("{:.2}", num);
                formatted
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string()
            }
        }
        n if n < 100.0 => {
            // 10-100：最多保留1位小数
            if n.fract() == 0.0 {
                format!("{:.0}", num)
            } else {
                let formatted = format!("{:.1}", num);
                formatted
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string()
            }
        }
        n if n < 10000.0 => {
            // 100-10000：保留整数
            format!("{:.0}", num)
        }
        n if n < 1_000_000.0 => {
            // 千单位 (k)
            let value = num / 1_000.0;
            format_with_unit(value, "k")
        }
        n if n < 1_000_000_000.0 => {
            // 百万单位 (M)
            let value = num / 1_000_000.0;
            format_with_unit(value, "M")
        }
        n if n < 1_000_000_000_000.0 => {
            // 十亿单位 (B)
            let value = num / 1_000_000_000.0;
            format_with_unit(value, "B")
        }
        n if n < 1_000_000_000_000_000.0 => {
            // 万亿单位 (T)
            let value = num / 1_000_000_000_000.0;
            format_with_unit(value, "T")
        }
        _ => {
            // 更大的数使用科学计数法
            format!("{:.2e}", num)
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

fn format_with_unit(value: f64, unit: &str) -> String {
    let abs_value = value.abs();

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
        // 对于大于100的值，取整数
        format!("{:.0}{}", value, unit)
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
