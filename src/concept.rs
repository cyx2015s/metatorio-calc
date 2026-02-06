use std::{any::Any, fmt::Debug, hash::Hash, sync::mpsc::*};

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
    type Game: Send + 'static;
    type Item: ItemIdent;
}

/// 能够在编辑器中展示自己的视图
pub trait EditorView: SolveContext {
    // 返回值表示是否产生了需要重新计算的更改
    fn editor_view(&mut self, ui: &mut egui::Ui, game: &Self::Game) -> bool;
}

pub type Flow<I> = IndexMap<I, f64>;

/// 能够转化成流参与计算的方法
pub trait AsFlow: SolveContext {
    /// 传递物品流信息
    fn as_flow(&self, game: &Self::Game) -> Flow<Self::Item>;
    /// 执行成本，默认返回 1.0
    fn cost(&self, game: &Self::Game) -> f64 {
        let _ = game;
        1.0
    }
}

pub type AsFlowSender<G, I> = Sender<Box<dyn AsFlow<Game = G, Item = I>>>;
pub type AsFlowReceiver<G, I> = Receiver<Box<dyn AsFlow<Game = G, Item = I>>>;
pub trait ItemIdent: Debug + Clone + Eq + Hash + Send + 'static {}
impl<T> ItemIdent for T where T: Debug + Clone + Eq + Hash + Send + 'static {}
pub trait GameContextCreatorView: Subview {
    fn set_subview_sender(&mut self, sender: Sender<Box<dyn Subview>>);
}

/// EditorView:  机制偏好编辑，而非机制实例编辑，每帧必须调用，在这一帧更新上一帧的所有操作
/// Mechanic:  机制，包含多个机制实例，且能够参与计算
pub trait Mechanic<G, I>:
    EditorView<Game = G, Item = I> + dyn_clone::DynClone + erased_serde::Serialize + Send
where
    G: Send + 'static,
    I: ItemIdent,
{
    fn name(&self) -> String;

    fn instances(&self) -> Vec<&dyn AsFlow<Game = G, Item = I>>;

    // 考虑提供一个更高效的实现。
    fn instance_len(&self) -> usize {
        self.instances().len()
    }

    // 获取某个实例的可变引用，以便进行编辑
    fn instance_operate(
        &mut self,
        idx: usize,
        f: &mut dyn FnMut(&mut dyn AsFlow<Game = G, Item = I>) -> EntryOperation,
    ) {
        let _ = idx;
        let _ = f;
    }

    // 提交所有instance_operate的更改
    // 返回值表示是否产生了需要重新计算的更改
    fn submit_operations(&mut self) -> bool;

    // 返回值表示是否产生了需要重新计算的更改
    fn instance_view(&mut self, idx: usize, ui: &mut egui::Ui, game: &G) -> bool {
        let _ = idx;
        let _ = ui;
        let _ = game;
        false
    }

    // 想要生产 amount 每秒数量的 item，有哪些方法？
    fn update_suggestion(&mut self, game: &G, item: &I, amount: f64) {
        let _ = game;
        let _ = item;
        let _ = amount;
    }

    // 返回值表示是否产生了需要重新计算的更改
    fn suggestion_view(&mut self, ui: &mut egui::Ui, game: &G) -> bool {
        let _ = ui;
        let _ = game;
        false
    }

    /// 自动规划功能：枚举所有可能的配方组合，填充到instances中。
    fn auto_populate(&mut self, game: &G) {
        let _ = game;
    }
}

pub trait WithUser {
    type User: Send + 'static;
}

pub trait MechanicWithUser<G, U, I>: Mechanic<G, I> + WithUser<User = U>
where
    G: Send + 'static,
    U: Send + 'static,
    I: ItemIdent,
{
    // 返回值表示是否产生了需要重新计算的更改
    fn instance_view_ext(&mut self, idx: usize, ui: &mut egui::Ui, game: &G, user: &U) -> bool {
        // 默认实现忽视 user
        let _ = user;
        self.instance_view(idx, ui, game)
    }

    // 返回值表示是否产生了需要重新计算的更改
    fn suggestion_view_ext(&mut self, ui: &mut egui::Ui, game: &G, user: &U) -> bool {
        // 默认实现忽视 user
        let _ = user;
        self.suggestion_view(ui, game)
    }

    fn update_suggestion_ext(&mut self, game: &G, user: &mut U, item: &I, amount: f64) {
        // 默认实现忽视 user
        let _ = user;
        self.update_suggestion(game, item, amount);
    }
}

dyn_clone::clone_trait_object!(<G, I> Mechanic<G, I> where G: Send + 'static, I : ItemIdent);
erased_serde::serialize_trait_object!(<G, I> Mechanic<G, I> where G: Send + 'static, I : ItemIdent);

dyn_clone::clone_trait_object!(<G, U, I> MechanicWithUser<G, U, I> where G: Send + 'static, U: Send + 'static, I : ItemIdent);
erased_serde::serialize_trait_object!(<G, U, I> MechanicWithUser<G, U, I> where G: Send + 'static, U: Send + 'static, I : ItemIdent);
