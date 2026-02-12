use std::{any::Any, fmt::Debug, hash::Hash, sync::mpsc::*};

use indexmap::IndexMap;

/// 对一个列表进行操作后，对这个项额外进行的操作，仅用作指示
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOpRequest {
    /// 无操作
    None,
    /// 删除当前项
    Drop,
    /// 复制当前项
    Clone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOpResult {
    Drop {
        removed: usize,
        replaced_by: Option<usize>,
    },
    Clone {
        original: usize,
        new: usize,
    },
}

pub trait SubView: Send {
    fn view(&mut self, ui: &mut egui::Ui);

    fn name(&self) -> String {
        "Subview".to_string()
    }

    fn description(&self) -> String {
        String::new()
    }
}

/// 解决方案上下文，包含游戏相关的信息
pub trait SolveContext: Debug + Send + Any {
    type Game: Send + 'static;
    type Item: ItemIdent;
}

pub type Flow<I> = IndexMap<I, f64>;

pub trait ItemIdent: Debug + Clone + Eq + Hash + Send + 'static {}
impl<T> ItemIdent for T where T: Debug + Clone + Eq + Hash + Send + 'static {}
pub trait GameContextCreatorView: SubView {
    fn set_subview_sender(&mut self, sender: Sender<Box<dyn SubView>>);
}