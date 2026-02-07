use std::ops::Deref;

use crate::concept::{EntryOpRequest, EntryOpResult};

pub enum DiffVecOpRequest {
    None,
    Clone,
    Remove,
}

pub struct DiffVecOpResult {
    pub idx: usize,
    pub replaced_by: Option<usize>,
}

pub struct DiffVec<T> {
    inner: Vec<T>,
}

impl<T> DiffVec<T> {
    pub fn new(inner: Vec<T>) -> Self {
        Self { inner }
    }

    #[must_use]
    pub fn push(&mut self, value: T) -> usize {
        let ret = self.inner.len();
        self.inner.push(value);
        ret
    }

    pub fn clone_at(&mut self, index: usize) -> usize
    where
        T: Clone,
    {
        let value = self.inner[index].clone();
        self.push(value)
    }

    #[must_use]
    pub fn remove_at(&mut self, index: usize) -> usize {
        self.inner.swap_remove(index);
        self.inner.len()
    }

    pub fn for_each<F>(&mut self, mut f: F) -> Vec<EntryOpResult>
    where
        F: FnMut(&mut T) -> EntryOpRequest,
        T: Clone,
    {
        let op_requests = self.inner.iter_mut().map(|v| f(v)).collect::<Vec<_>>();
        let mut results = Vec::new();
        for i in (0..op_requests.len()).rev() {
            match op_requests[i] {
                EntryOpRequest::None => {}
                EntryOpRequest::Clone => {
                    let new_idx = self.clone_at(i);

                    results.push(EntryOpResult::Clone {
                        original: i,
                        new: new_idx,
                    });
                }
                EntryOpRequest::Drop => {
                    let replaced_by = self.remove_at(i);
                    if replaced_by > self.inner.len() {
                        // 没有被替换，删掉的是最后一个元素
                        results.push(EntryOpResult::Drop {
                            removed: i,
                            replaced_by: None,
                        });
                    } else {
                        // 被替换了，记录替换者的索引
                        results.push(EntryOpResult::Drop {
                            removed: i,
                            replaced_by: Some(replaced_by),
                        });
                    }
                }
            }
        }
        results
    }
}

impl<T> Deref for DiffVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[test]
fn test_diff_vec() {
    let mut dv = DiffVec::new(vec![1, 2, 3]);
    assert_eq!(dv.len(), 3);
    assert_eq!(dv[0], 1);
    dbg!(dv.push(4));
    dbg!(dv.clone_at(1));
    assert_eq!(dv.len(), 5);
}

#[test]
fn test_diff_vec_ord() {
    fn test_with_n(n: usize) {
        println!("n={}", n);
        let mut dv = DiffVec::new((0..n).collect::<Vec<_>>());
        let mut idx = (0..dv.len()).collect::<Vec<_>>();
        let results = dv.for_each(|v| {
            if *v % 2 == 0 {
                EntryOpRequest::Drop
            } else if *v % 3 == 0 {
                EntryOpRequest::Clone
            } else {
                EntryOpRequest::None
            }
        });

        for result in results {
            match result {
                EntryOpResult::Drop {
                    removed,
                    replaced_by,
                } => {
                    println!("removed: {}, replaced by: {:?}", removed, replaced_by);
                    idx.retain_mut(|i| {
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
                    println!("cloned: {}, new index: {}", original, new);
                    for i in (0..idx.len()).rev() {
                        if idx[i] == original {
                            idx.insert(i + 1, new);
                            break;
                        }
                    }
                }
            }
        }

        let result_vec = idx.iter().map(|&i| dv[i]).collect::<Vec<_>>();
        for v in &result_vec {
            assert!(*v % 2 != 0);
        }
        for window in result_vec.windows(3) {
            assert!(window[0] <= window[1]);
            if window[1] % 3 == 0 {
                assert!(window[0] == window[1] || window[2] == window[1]);
            }
        }
        println!("final vector: {:?}", result_vec);
    }

    test_with_n(10);
    test_with_n(20);
}
