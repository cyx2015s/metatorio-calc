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
    // 返回值表示是否产生了需要重新计算的更改
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::GameContext) -> bool;
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

pub type MechanicSender<C, I> =
    std::sync::mpsc::Sender<Box<dyn MechanicInstance<GameContext = C, ItemIdentType = I>>>;
pub type MechanicReceiver<C, I> =
    std::sync::mpsc::Receiver<Box<dyn MechanicInstance<GameContext = C, ItemIdentType = I>>>;
pub trait ItemIdent: Debug + Clone + Eq + Hash + Send + 'static {}
impl<T> ItemIdent for T where T: Debug + Clone + Eq + Hash + Send + 'static {}
pub trait GameContextCreatorView: Subview {
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn Subview>>);
}

pub trait MechanicInstance:
    AsFlow + EditorView + dyn_clone::DynClone + erased_serde::Serialize
{
}

impl<T> MechanicInstance for T where
    T: AsFlow + EditorView + dyn_clone::DynClone + erased_serde::Serialize
{
}

erased_serde::serialize_trait_object!(<C, I> MechanicInstance<GameContext = C, ItemIdentType = I>);

dyn_clone::clone_trait_object!(<C, I> MechanicInstance<GameContext = C, ItemIdentType = I>);

pub trait MechanicProvider:
    EditorView + SolveContext + dyn_clone::DynClone + erased_serde::Serialize
{
    /// 传递创建的配方信息
    fn set_mechanic_sender(
        &mut self,
        sender: MechanicSender<Self::GameContext, Self::ItemIdentType>,
    );

    fn with_mechanic_sender(
        mut self,
        sender: MechanicSender<Self::GameContext, Self::ItemIdentType>,
    ) -> Self
    where
        Self: Sized,
    {
        self.set_mechanic_sender(sender);
        self
    }

    /// TODO
    /// 游戏机制提供器可选：自动填充逻辑
    fn auto_populate(
        &self,
        _ctx: &Self::GameContext,
        _flows: &HashMap<usize, Flow<Self::ItemIdentType>>,
    ) -> Vec<
        Box<
            dyn MechanicInstance<
                    ItemIdentType = Self::ItemIdentType,
                    GameContext = Self::GameContext,
                >,
        >,
    > {
        // 默认不实现任何自动填充逻辑
        vec![]
    }

    /// 在规划界面点击物品时，可以提供一些推荐配方
    fn hint_populate(
        &self,
        _ctx: &Self::GameContext,
        _item: &Self::ItemIdentType,
        _value: f64,
    ) -> Vec<
        Box<
            dyn MechanicInstance<
                    ItemIdentType = Self::ItemIdentType,
                    GameContext = Self::GameContext,
                >,
        >,
    > {
        vec![]
    }
}

dyn_clone::clone_trait_object!(<C, I> MechanicProvider<GameContext = C, ItemIdentType = I>);

erased_serde::serialize_trait_object!(<C, I> MechanicProvider<GameContext = C, ItemIdentType = I>);

/// EditorView:  机制偏好编辑，而非机制实例编辑
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

    fn instances(&self) -> Vec<&dyn MechanicInstance<GameContext = C, ItemIdentType = I>>;

    fn instances_mut(
        &mut self,
    ) -> Vec<&mut dyn MechanicInstance<GameContext = C, ItemIdentType = I>>;

    /// 想要生产 amount 每秒数量的 item，有哪些方法？
    fn update_suggestion(&mut self, ctx: &C, item: &I, amount: f64);

    /// 返回值表示是否产生了需要重新计算的更改
    fn suggestion_view(&mut self, ui: &mut egui::Ui, ctx: &C) -> bool {
        let _ = ui;
        let _ = ctx;
        false
    }

    /// 自动填充功能，用于自动规划模式下生成所有可能的机制实例
    fn auto_populate(
        &mut self,
        ctx: &C,
        sender: MechanicSender<C, I>, // 传递的所有物品流信息
    ) {
        let _ = ctx;
        let _ = sender;
    }

    fn get_instance_sender(
        &self,
    ) -> Option<&MechanicSender<C, I>> {
        let _ = self;
        None
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