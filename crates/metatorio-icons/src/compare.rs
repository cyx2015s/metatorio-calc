//! 与游戏官方导出结果比对（本 crate 的验收手段）。
//!
//! **alpha 口径**：游戏 `--dump-icon-sprites` 导出的是**预乘 alpha**，我们渲染的是
//! 直通 alpha，所以比对前先把我们这边预乘一次（`our.premultiplied()`）——直接比原始
//! 值会把「预乘」误判成大量差异（实测就是这样才发现导出是预乘的）。

use crate::image::Rgba8;

/// 一张图的比对结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffStats {
    /// 两侧尺寸不同（只能比交集）。
    pub size_mismatch: bool,
    /// 参与比较的像素数。
    pub pixels: usize,
    /// 通道差 ≤ `tolerance` 的像素数。
    pub matching: usize,
    /// 最大通道差。
    pub max_delta: u8,
    /// 差得最多的像素坐标（用于人工复核）。
    pub worst_at: (u32, u32),
}

impl DiffStats {
    pub fn is_exact(&self) -> bool {
        self.matching == self.pixels
    }

    /// 匹配率（0~1）。
    pub fn match_ratio(&self) -> f64 {
        if self.pixels == 0 {
            1.0
        } else {
            self.matching as f64 / self.pixels as f64
        }
    }
}

/// 逐像素比较（`ours` 会先预乘，`reference` 是官方导出的预乘图）。
pub fn diff_against_official(ours: &Rgba8, reference: &Rgba8, tolerance: u8) -> DiffStats {
    let ours = ours.premultiplied();
    let width = ours.width.min(reference.width);
    let height = ours.height.min(reference.height);
    let mut stats = DiffStats {
        size_mismatch: ours.width != reference.width || ours.height != reference.height,
        pixels: (width as usize) * (height as usize),
        matching: 0,
        max_delta: 0,
        worst_at: (0, 0),
    };
    for y in 0..height {
        for x in 0..width {
            let a = ours.pixel(x, y);
            let b = reference.pixel(x, y);
            let delta = (0..4)
                .map(|index| a[index].abs_diff(b[index]))
                .max()
                .unwrap_or(0);
            if delta > stats.max_delta {
                stats.max_delta = delta;
                stats.worst_at = (x, y);
            }
            if delta <= tolerance {
                stats.matching += 1;
            }
        }
    }
    stats
}

/// 一组图标的汇总（比对工具打印这个）。
#[derive(Debug, Default, Clone)]
pub struct CompareReport {
    /// 参与比对的原型数。
    pub total: usize,
    /// 渲染本身失败（缺文件/解码失败/没有图标定义）的数量。
    pub render_failed: usize,
    /// 参考图缺失（官方没导出这个原型的图标）的数量。
    pub reference_missing: usize,
    /// 逐像素完全一致的数量。
    pub exact: usize,
    /// 匹配率之和（用于算平均匹配率）。
    pub match_ratio_sum: f64,
    /// 最大通道差的最大值。
    pub worst_delta: u8,
    /// 最差的一批（原型名、匹配率、最大差）——便于人工复核。
    pub worst: Vec<(String, f64, u8)>,
}

impl CompareReport {
    pub fn compared(&self) -> usize {
        self.total - self.render_failed - self.reference_missing
    }

    pub fn average_match_ratio(&self) -> f64 {
        if self.compared() == 0 {
            1.0
        } else {
            self.match_ratio_sum / self.compared() as f64
        }
    }

    pub fn record(&mut self, name: &str, stats: &DiffStats) {
        self.total += 1;
        self.match_ratio_sum += stats.match_ratio();
        if stats.max_delta > self.worst_delta {
            self.worst_delta = stats.max_delta;
        }
        if stats.is_exact() {
            self.exact += 1;
        }
        self.worst
            .push((name.to_string(), stats.match_ratio(), stats.max_delta));
        self.worst.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.2.cmp(&a.2))
        });
        self.worst.truncate(10);
    }

    pub fn record_render_failure(&mut self) {
        self.total += 1;
        self.render_failed += 1;
    }

    pub fn record_missing_reference(&mut self) {
        self.total += 1;
        self.reference_missing += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_reference_matches_straight_source() {
        // 源（直通）与「官方导出」（预乘）在预乘后应当一致。
        let ours = Rgba8::from_pixels(1, 1, vec![200, 100, 50, 128]);
        let official = ours.premultiplied();
        let stats = diff_against_official(&ours, &official, 0);
        assert!(stats.is_exact(), "{stats:?}");
        assert_eq!(stats.max_delta, 0);
    }

    #[test]
    fn tolerance_counts_near_misses() {
        let ours = Rgba8::from_pixels(1, 1, vec![100, 100, 100, 255]);
        let reference = Rgba8::from_pixels(1, 1, vec![102, 100, 100, 255]);
        assert_eq!(diff_against_official(&ours, &reference, 0).matching, 0);
        assert_eq!(diff_against_official(&ours, &reference, 2).matching, 1);
        assert_eq!(diff_against_official(&ours, &reference, 2).max_delta, 2);
    }
}
