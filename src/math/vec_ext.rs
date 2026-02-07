
use crate::concept::{EntryOpRequest, EntryOpResult};

pub trait ElemVec<T> {
    fn update_elements(
        &mut self,
        operations: &mut Vec<(usize, EntryOpRequest)>,
    ) -> Vec<EntryOpResult>
    where
        T: Clone;
}

impl<T> ElemVec<T> for Vec<T> {
    fn update_elements(
        &mut self,
        operations: &mut Vec<(usize, EntryOpRequest)>,
    ) -> Vec<EntryOpResult>
    where
        T: Clone,
    {
        operations.sort_by_key(|(idx, _)| *idx);
        let mut results = Vec::new();
        for (idx, op) in operations.iter().rev() {
            match op {
                EntryOpRequest::None => {}
                EntryOpRequest::Clone => {
                    let value = self[*idx].clone();
                    let new_idx = self.len();
                    self.push(value);
                    results.push(EntryOpResult::Clone {
                        original: *idx,
                        new: new_idx,
                    });
                }
                EntryOpRequest::Drop => {
                    let replaced_by = self.len();
                    self.swap_remove(*idx);
                    if replaced_by > self.len() {
                        // 没有被替换，删掉的是最后一个元素
                        results.push(EntryOpResult::Drop {
                            removed: *idx,
                            replaced_by: None,
                        });
                    } else {
                        // 被替换了，记录替换者的索引
                        results.push(EntryOpResult::Drop {
                            removed: *idx,
                            replaced_by: Some(replaced_by),
                        });
                    }
                }
            }
        }
        operations.clear();
        results
    }
}

pub fn update_indexes(results: Vec<EntryOpResult>, indexes: &mut Vec<usize>) -> bool {
    let mut changed = false;
    for result in results {
        match result {
            EntryOpResult::Drop {
                removed,
                replaced_by,
            } => {
                changed = true;
                indexes.retain_mut(|i| {
                    if *i == removed {
                        return false;
                    }
                    if Some(*i) == replaced_by {
                        *i = removed;
                    }
                    return true;
                });
            }
            EntryOpResult::Clone { original, new } => {
                changed = true;
                for i in (0..indexes.len()).rev() {
                    if indexes[i] == original {
                        indexes.insert(i + 1, new);
                        break;
                    }
                }
            }
        }
    }
    changed
}