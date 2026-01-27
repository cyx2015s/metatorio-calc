use std::{any::Any, collections::HashMap, fmt::Debug, hash::Hash};

use indexmap::IndexMap;

pub trait Subview: Send {
    fn view(&mut self, ui: &mut egui::Ui);

    fn name(&self) -> String {
        "Subview".to_string()
    }

    fn description(&self) -> String {
        String::new()
    }
}

pub trait SolveContext: Send + Any {
    type GameContext;
    type ItemIdentType: ItemIdent;
}

pub trait EditorView: SolveContext {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::GameContext);
}

pub type Flow<I> = IndexMap<I, f64>;

pub trait AsFlow: SolveContext {
    /// 传递物品流信息
    fn as_flow(&self, ctx: &Self::GameContext) -> Flow<Self::ItemIdentType>;
    /// 执行成本，默认返回 1.0
    fn cost(&self, _ctx: &Self::GameContext) -> f64 {
        1.0
    }
}

pub type MechanicSender<I, C> =
    std::sync::mpsc::Sender<Box<dyn Mechanic<ItemIdentType = I, GameContext = C>>>;

pub trait ItemIdent: Debug + Clone + Eq + Hash + 'static {}


pub trait GameContextCreatorView: Subview {
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn Subview>>);
}

pub trait Mechanic: AsFlow + EditorView + dyn_clone::DynClone + erased_serde::Serialize {}

impl<T> Mechanic for T where T: AsFlow + EditorView + dyn_clone::DynClone + erased_serde::Serialize {}

erased_serde::serialize_trait_object!(<C, I> Mechanic<GameContext = C, ItemIdentType = I>);

dyn_clone::clone_trait_object!(<C, I> Mechanic<GameContext = C, ItemIdentType = I>);

pub trait MechanicProvider: EditorView + SolveContext + dyn_clone::DynClone {
    /// 传递创建的配方信息
    fn set_mechanic_sender(
        &mut self,
        sender: MechanicSender<Self::ItemIdentType, Self::GameContext>,
    );

    /// TODO
    /// 游戏机制提供器可选：自动填充逻辑
    fn auto_populate(
        &self,
        _ctx: &Self::GameContext,
        _flows: &HashMap<usize, Flow<Self::ItemIdentType>>,
    ) -> Vec<Box<dyn Mechanic<ItemIdentType = Self::ItemIdentType, GameContext = Self::GameContext>>>
    {
        // 默认不实现任何自动填充逻辑
        vec![]
    }

    /// 在规划界面点击物品时，可以提供一些推荐配方
    fn hint_populate(
        &self,
        _ctx: &Self::GameContext,
        _item: &Self::ItemIdentType,
        _value: f64,
    ) -> Vec<Box<dyn Mechanic<ItemIdentType = Self::ItemIdentType, GameContext = Self::GameContext>>>
    {
        vec![]
    }
}

dyn_clone::clone_trait_object!(<C, I> MechanicProvider<GameContext = C, ItemIdentType = I>);
