use std::{any::Any, fmt::Debug, sync::mpsc::*};

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
pub trait SolveContext: Debug + Send + Sync + Any {
    type Game: Send + 'static;
    type Item: ItemIdent;
}

// 求解器内核的最小概念集（物品标识/流量集合）已随内核拆入 `metatorio-solver`，
// 此处 re-export，保持 `crate::concept::{AIndexMap, ...}` 的既有引用不变。
pub use metatorio_solver::concept::{AIndexMap, AIndexSet, Flow, ItemIdent};

pub trait GameContextCreatorView: SubView {
    fn set_subview_sender(&mut self, sender: Sender<Box<dyn SubView>>);
}
