use serde::ser::SerializeSeq;

use crate::concept::{EntryOpRequest, EntryOpResult};

pub trait UpdateVec<T> {
    fn update_elements(
        &mut self,
        operations: &mut Vec<(usize, EntryOpRequest)>,
    ) -> Vec<EntryOpResult>
    where
        T: Clone;
}

impl<T> UpdateVec<T> for Vec<T> {
    fn update_elements(
        &mut self,
        operations: &mut Vec<(usize, EntryOpRequest)>,
    ) -> Vec<EntryOpResult>
    where
        T: Clone,
    {
        operations.sort_by_key(|(idx, _)| *idx);
        let mut results = Vec::new();
        let mut last_idx = None;
        for (idx, op) in operations.iter().rev() {
            if let Some(last_idx) = last_idx
                && *idx >= last_idx
            {
                // 已经被删除或克隆过，跳过
                continue;
            }
            last_idx = Some(*idx);
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
                    self.swap_remove(*idx);
                    if *idx >= self.len() {
                        // 没有被替换，删掉的是最后一个元素
                        results.push(EntryOpResult::Drop {
                            removed: *idx,
                            replaced_by: None,
                        });
                    } else {
                        // 被替换了，记录替换者的索引
                        results.push(EntryOpResult::Drop {
                            removed: *idx,
                            replaced_by: Some(self.len()),
                        });
                    }
                }
            }
        }
        operations.clear();
        results
    }
}

// 物品本身不方便移动的时候，用索引来移动
#[derive(Debug, Default)]
pub struct DndVec<T> {
    pub vec: Vec<T>,
    pub idx: Vec<usize>,
}

impl<T> Clone for DndVec<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            vec: self.vec.clone(),
            idx: self.idx.clone(),
        }
    }
}

impl<T> DndVec<T> {
    pub fn new() -> Self {
        Self {
            vec: Vec::new(),
            idx: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.vec.clear();
        self.idx.clear();
    }

    pub fn insert(&mut self, index: usize, value: T) {
        self.vec.push(value);
        let new_idx = self.vec.len() - 1;
        self.idx.insert(index, new_idx);
    }

    pub fn push(&mut self, value: T) {
        self.vec.push(value);
        self.idx.push(self.vec.len() - 1);
    }

    pub fn remove(&mut self, index: usize) -> T {
        debug_assert!(index < self.idx.len());
        let swap_target = self.idx.len() - 1;
        let swap_index = self
            .idx
            .iter()
            .enumerate()
            .find(|(_, b)| **b == swap_target)
            .unwrap()
            .0;
        self.idx.swap(index, swap_index);
        let ret = self.vec.swap_remove(self.idx[swap_index]);
        self.idx.remove(index);
        ret
    }

    pub fn len(&self) -> usize {
        self.idx.len()
    }

    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        IndexedVecIter {
            indexed_vec: self,
            current: 0,
        }
    }
}

struct IndexedVecIter<'a, T> {
    indexed_vec: &'a DndVec<T>,
    current: usize,
}

impl<'a, T> Iterator for IndexedVecIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.indexed_vec.idx.len() {
            None
        } else {
            let item = &self.indexed_vec.vec[self.indexed_vec.idx[self.current]];
            self.current += 1;
            Some(item)
        }
    }
}

impl<T> std::ops::Index<usize> for DndVec<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        let real_index = self.idx[index];
        &self.vec[real_index]
    }
}

impl<T> std::ops::IndexMut<usize> for DndVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let real_index = self.idx[index];
        &mut self.vec[real_index]
    }
}

impl<T> From<Vec<T>> for DndVec<T> {
    fn from(value: Vec<T>) -> Self {
        let idx = (0..value.len()).collect();
        Self { vec: value, idx }
    }
}

impl<T> From<DndVec<T>> for Vec<T> {
    fn from(value: DndVec<T>) -> Self {
        value.vec
    }
}

impl<T> serde::Serialize for DndVec<T>
where
    T: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.idx.len()))?;
        for &i in &self.idx {
            seq.serialize_element(&self.vec[i])?;
        }
        seq.end()
    }
}

impl<'a, T> serde::Deserialize<'a> for DndVec<T>
where
    T: serde::Deserialize<'a>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        let vec: Vec<T> = serde::Deserialize::deserialize(deserializer)?;
        let idx = (0..vec.len()).collect();
        Ok(DndVec { vec, idx })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_indexed_vec_serialize() {
        let mut indexed_vec = DndVec::new();
        indexed_vec.push(10);
        indexed_vec.push(20);
        indexed_vec.push(30);
        indexed_vec.idx.swap(0, 2); // 改变索引顺序

        let serialized = serde_json::to_string(&indexed_vec).unwrap();
        assert_eq!(serialized, "[30,20,10]");

        let deserialized: DndVec<i32> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.vec, vec![30, 20, 10]);
        assert_eq!(deserialized.idx, vec![0, 1, 2]);
    }
    #[test]
    fn test_vec_behavior() {
        let mut plain_vec: Vec<i32> = (10..20).collect();
        let mut indexed_vec = DndVec::from(plain_vec.clone());
        indexed_vec.remove(2);
        plain_vec.remove(2);
        assert_eq!(indexed_vec.iter().cloned().collect::<Vec<_>>(), plain_vec);

        indexed_vec.push(100);
        plain_vec.push(100);
        assert_eq!(indexed_vec.iter().cloned().collect::<Vec<_>>(), plain_vec);

        indexed_vec.remove(0);
        plain_vec.remove(0);
        assert_eq!(indexed_vec.iter().cloned().collect::<Vec<_>>(), plain_vec);
    }
}
