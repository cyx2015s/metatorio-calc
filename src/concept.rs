use std::{any::Any, fmt::Debug, hash::Hash};

use indexmap::IndexMap;

/// 对一个列表进行操作后，对这个项额外进行的操作，仅用作指示
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOperation {
    /// 无操作
    None,
    /// 删除当前项
    Drop,
    /// 复制当前项
    Clone,
}

pub trait Subview: Send {
    fn view(&mut self, ui: &mut egui::Ui);

    fn name(&self) -> String {
        "Subview".to_string()
    }

    fn description(&self) -> String {
        String::new()
    }
}

/// 解决方案上下文，包含游戏相关的信息
pub trait SolveContext: Send + Any {
    type GameContext;
    type ItemIdentType: ItemIdent;
}

/// 能够在编辑器中展示自己的视图
pub trait EditorView: SolveContext {
    // 返回值表示是否产生了需要重新计算的更改
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::GameContext) -> bool;
}

pub type Flow<I> = IndexMap<I, f64>;

/// 能够转化成流参与计算的方法
pub trait AsFlow: SolveContext {
    /// 传递物品流信息
    fn as_flow(&self, ctx: &Self::GameContext) -> Flow<Self::ItemIdentType>;
    /// 执行成本，默认返回 1.0
    fn cost(&self, _ctx: &Self::GameContext) -> f64 {
        1.0
    }
}

pub type AsFlowSender<C, I> =
    std::sync::mpsc::Sender<Box<dyn AsFlow<GameContext = C, ItemIdentType = I>>>;
pub type AsFlowReceiver<C, I> =
    std::sync::mpsc::Receiver<Box<dyn AsFlow<GameContext = C, ItemIdentType = I>>>;
pub trait ItemIdent: Debug + Clone + Eq + Hash + Send + 'static {}
impl<T> ItemIdent for T where T: Debug + Clone + Eq + Hash + Send + 'static {}
pub trait GameContextCreatorView: Subview {
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn Subview>>);
}

/// EditorView:  机制偏好编辑，而非机制实例编辑，每帧必须调用，在这一帧更新上一帧的所有操作
/// Mechanic:  机制，包含多个机制实例，且能够参与计算
pub trait Mechanic<C, I>:
    EditorView<GameContext = C, ItemIdentType = I>
    + dyn_clone::DynClone
    + erased_serde::Serialize
    + Send
where
    C: Send + 'static,
    I: ItemIdent,
{
    fn name(&self) -> String;

    fn instances(&self) -> Vec<&dyn AsFlow<GameContext = C, ItemIdentType = I>>;

    // 考虑提供一个更高效的实现。
    fn instance_len(&self) -> usize {
        self.instances().len()
    }

    // 获取某个实例的可变引用，以便进行编辑
    fn instance_operate(
        &mut self,
        idx: usize,
        f: &mut dyn FnMut(&mut dyn AsFlow<GameContext = C, ItemIdentType = I>) -> EntryOperation,
    );

    // 提交所有instance_operate的更改
    // 返回值表示是否产生了需要重新计算的更改
    fn submit_operations(&mut self) -> bool;

    // 返回值表示是否产生了需要重新计算的更改
    fn instance_view(&mut self, idx: usize, ui: &mut egui::Ui, ctx: &C) -> bool;

    // 想要生产 amount 每秒数量的 item，有哪些方法？
    fn update_suggestion(&mut self, ctx: &C, item: &I, amount: f64);

    // 返回值表示是否产生了需要重新计算的更改
    fn suggestion_view(&mut self, ui: &mut egui::Ui, ctx: &C) -> bool {
        let _ = ui;
        let _ = ctx;
        false
    }

    /// 自动规划功能：枚举所有可能的配方组合，填充到instances中。
    fn auto_populate(
        &mut self,
        ctx: &C,
    ) {
        let _ = ctx;
    }
}

dyn_clone::clone_trait_object!(<C, I> Mechanic<C, I> where C: Send + 'static, I : ItemIdent);
erased_serde::serialize_trait_object!(<C, I> Mechanic<C, I> where C: Send + 'static, I : ItemIdent);

pub trait PlannerView<C, I>: erased_serde::Serialize + dyn_clone::DynClone
where
    C: Send + 'static,
    I: ItemIdent,
{
    /// 所有的游戏机制，偏好设置和实例化结果
    fn mechanics(&self) -> &[Box<dyn Mechanic<C, I>>];

    fn mechanics_mut(&mut self) -> &mut [Box<dyn Mechanic<C, I>>];

    /// 所有游戏机制展示时的顺序
    fn mechanics_index(&self) -> &[(usize, usize)]; // (mechanic_idx, instance_idx)

    fn mechanics_index_mut(&mut self) -> &mut [(usize, usize)];

    /// 规划目标和产量
    fn targets(&self) -> &[(I, f64)];

    fn targets_mut(&mut self) -> &mut [(I, f64)];

    /// 外界输入和代价
    fn externals(&self) -> &[(I, f64)];

    fn externals_mut(&mut self) -> &mut [(I, f64)];

    /// 返回值表示是否产生了需要重新计算的更改，用于保存
    fn planner_view(&mut self, ui: &mut egui::Ui, ctx: &C) -> bool;
}

dyn_clone::clone_trait_object!(<C, I> PlannerView<C, I> where C: Send + 'static, I : ItemIdent);
erased_serde::serialize_trait_object!(<C, I> PlannerView<C, I> where C: Send + 'static, I : ItemIdent);
