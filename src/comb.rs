/// 迭代器：生成 n 个非负整数之和等于 m 的所有组合
pub struct Compositions {
    state: Vec<usize>,
    n: usize,
    finished: bool,
}

impl Compositions {
    /// 创建新的组合迭代器
    ///
    /// # 参数
    /// * `n` - 整数个数（必须 > 0）
    /// * `m` - 目标和
    pub fn new(n: usize, m: usize) -> Self {
        if n == 0 {
            return Self {
                state: vec![],
                n: 0,
                finished: m != 0, // n=0 且 m>0 时无解；n=0 且 m=0 时只有空组合
            };
        }
        // 初始状态：[0, 0, ..., m]（前 n-1 个为 0，最后一个为 m）
        Self {
            state: {
                let mut v = vec![0; n];
                if n > 0 {
                    v[n - 1] = m;
                }
                v
            },
            n,
            finished: false,
        }
    }
}

impl Iterator for Compositions {
    type Item = Vec<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished || self.n == 0 {
            if self.n == 0 && !self.finished {
                // 特殊情况：n=0 且 m=0，只返回一次空向量
                self.finished = true;
                return Some(vec![]);
            }
            return None;
        }

        // 克隆当前状态作为返回值
        let result = Some(self.state.clone());

        // 生成下一个组合（字典序）
        // 算法：从右向左找到第一个可以「进位」的位置
        let mut sum_right = 0;
        let mut found = false;

        // 从倒数第二个位置开始向左扫描（最右侧位置不能作为进位源）
        for i in (0..self.n - 1).rev() {
            sum_right += self.state[i + 1];
            if sum_right > 0 {
                // 可以从右侧借 1 个单位到位置 i
                self.state[i] += 1;
                sum_right -= 1;

                // 将 i+1 之后的位置重置：中间填 0，剩余值放最右侧
                for j in i + 1..self.n - 1 {
                    self.state[j] = 0;
                }
                self.state[self.n - 1] = sum_right;
                found = true;
                break;
            }
        }

        if !found {
            self.finished = true;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::Compositions;

    #[test]
    fn test_compositions() {
        let comps: Vec<Vec<usize>> = Compositions::new(3, 4).collect();
        let expected = vec![
            vec![0, 0, 4],
            vec![0, 1, 3],
            vec![0, 2, 2],
            vec![0, 3, 1],
            vec![0, 4, 0],
            vec![1, 0, 3],
            vec![1, 1, 2],
            vec![1, 2, 1],
            vec![1, 3, 0],
            vec![2, 0, 2],
            vec![2, 1, 1],
            vec![2, 2, 0],
            vec![3, 0, 1],
            vec![3, 1, 0],
            vec![4, 0, 0],
        ];
        assert_eq!(comps, expected);
    }

    #[test]
    fn test_0_0() {
        let comps: Vec<Vec<usize>> = Compositions::new(0, 0).collect();
        let expected: Vec<Vec<usize>> = vec![vec![]];
        assert_eq!(comps, expected);
    }
    #[test]
    fn test_0_1() {
        let comps: Vec<Vec<usize>> = Compositions::new(0, 1).collect();
        let expected: Vec<Vec<usize>> = vec![];
        assert_eq!(comps, expected);
    }
    #[test]
    fn test_1_0() {
        let comps: Vec<Vec<usize>> = Compositions::new(1, 0).collect();
        let expected: Vec<Vec<usize>> = vec![vec![0]];
        assert_eq!(comps, expected);
    }
}
