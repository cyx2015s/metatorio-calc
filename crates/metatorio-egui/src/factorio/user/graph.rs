use std::collections::VecDeque;

use crate::concept::{AIndexMap, AIndexSet};
use crate::factorio::{DataContext, Dict, Modifier, ProjectContext, RecipeResult};

#[derive(Debug, Clone, Default)]
pub struct MilestoneNode {
    pub depth: usize,              // depth表示到这个里程碑的最深科技深度
    pub name: String,              // 里程碑名称
    pub dependencies: Vec<String>, // 里程碑直接依赖的里程碑名称列表
}

pub fn resolve_milestone_graph(
    data: &DataContext,
    milestones: &[(String, bool)],
) -> AIndexMap<String, MilestoneNode> {
    let mut ret = AIndexMap::default();

    let mut queue = data
        .technologies
        .iter()
        .filter_map(|(name, proto)| {
            if proto.prerequisites.is_empty() {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect::<VecDeque<_>>();

    let mut visited: AIndexSet<&str> = AIndexSet::default();

    log::debug!("初始科技起点: {:?}", queue);
    log::debug!("里程碑: {:?}", milestones);
    #[derive(Debug, Clone, Default)]
    struct NodeInfo<'a> {
        indeg: usize,
        deps: AIndexSet<&'a str>,
    }

    let mut node_infos = data
        .technologies
        .iter()
        .map(|(name, tech)| {
            (
                name.as_str(),
                NodeInfo {
                    indeg: tech.prerequisites.len(),
                    deps: AIndexSet::default(),
                },
            )
        })
        .collect::<AIndexMap<&str, NodeInfo>>();

    while let Some(current_tech) = queue.pop_front() {
        if visited.contains(current_tech) {
            continue;
        }
        log::debug!("处理科技 {}", current_tech);
        visited.insert(current_tech);
        let depth = data.technologies.get(current_tech).map_or(0, |t| {
            t.prerequisites
                .iter()
                .filter_map(|prereq| {
                    ret.get(prereq.as_str())
                        .map(|node: &MilestoneNode| node.depth)
                })
                .max()
                .unwrap_or(0)
                + 1
        });

        let current_node = node_infos.get(current_tech).cloned().unwrap_or_default();
        let current_is_milestone = milestones.iter().any(|(name, _)| name == current_tech);
        if current_is_milestone {
            // 只有出现在里程碑中的科技才会被加入到里程碑图中。
            log::debug!(
                "科技 {} 是里程碑，添加到里程碑图中，深度 {}, 可能依赖 {:?}",
                current_tech,
                depth,
                current_node.deps
            );
            ret.insert(
                current_tech.to_string(),
                MilestoneNode {
                    depth,
                    name: current_tech.to_string(),
                    dependencies: current_node.deps.iter().map(|v| v.to_string()).collect(),
                },
            );
        }
        if let Some(dependents) = data.technology_dependents.get(current_tech) {
            for dependent in dependents {
                log::debug!(
                    "科技 {} 的后续科技 {}，降低入度，剩余入度 {}",
                    current_tech,
                    dependent,
                    node_infos
                        .get(dependent.as_str())
                        .map_or(0, |info| info.indeg.saturating_sub(1))
                );
                if let Some(NodeInfo { indeg, deps }) = node_infos.get_mut(dependent.as_str()) {
                    *indeg -= 1;
                    deps.extend(current_node.deps.clone());
                    if *indeg == 0 {
                        queue.push_back(dependent.as_str());
                    }
                    if current_is_milestone {
                        deps.insert(current_tech);
                    }
                }
            }
        }
    }

    // 从半成品依赖图中推算真实依赖图

    transitive_reduction_and_build_depth(&ret)
}

pub fn transitive_reduction_and_build_depth(
    graph: &AIndexMap<String, MilestoneNode>,
) -> AIndexMap<String, MilestoneNode> {
    // 建立名称到索引的映射
    let mut name_to_idx: AIndexMap<&str, usize> = AIndexMap::default();
    let mut idx_to_name: Vec<&str> = Vec::new();
    for name in graph.keys() {
        name_to_idx.insert(name, idx_to_name.len());
        idx_to_name.push(name);
    }
    let n = name_to_idx.len();

    // 构建原始邻接矩阵
    let mut adj = vec![vec![false; n]; n];
    for (name, node) in graph {
        let i = name_to_idx[name.as_str()];
        for dep in &node.dependencies {
            if let Some(&j) = name_to_idx.get(dep.as_str()) {
                adj[i][j] = true;
            } else {
                log::error!("依赖项 {} 不存在", dep);
            }
        }
    }

    // Floyd-Warshall 计算传递闭包
    let mut closure = adj.clone();
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                if closure[i][k] && closure[k][j] {
                    closure[i][j] = true;
                }
            }
        }
    }

    // 构建约简后的图
    let mut reduced = AIndexMap::default();
    for (name, node) in graph {
        let i = name_to_idx[name.as_str()];
        let mut new_deps = Vec::new();
        for dep in &node.dependencies {
            if let Some(&j) = name_to_idx.get(dep.as_str()) {
                // 检查是否存在中间节点 k (k != i, k != j) 使得 i -> k 且 k -> j
                let mut redundant = false;
                #[allow(clippy::needless_range_loop)]
                for k in 0..n {
                    if k != i && k != j && closure[i][k] && closure[k][j] {
                        redundant = true;
                        break;
                    }
                }
                if !redundant {
                    new_deps.push(dep.clone());
                }
            } else {
                // 保留不在图中的依赖（可能为外部节点）
                new_deps.push(dep.clone());
            }
        }
        let mut new_node = node.clone();

        new_node.dependencies = new_deps;
        reduced.insert(name.clone(), new_node);
    }

    let mut tech_depth = graph
        .values()
        .map(|t| (t.name.as_str(), 0))
        .collect::<AIndexMap<_, _>>();

    for _ in 0..reduced.len() {
        // ……我不想优化了，循环N次总能得到深度的
        for tech_name in graph.keys() {
            let tech = graph.get(tech_name).unwrap();
            for dep in &tech.dependencies {
                let dep_depth = tech_depth.get(dep.as_str()).cloned().unwrap_or(0);
                tech_depth
                    .entry(tech_name)
                    .and_modify(|d| *d = (*d).max(dep_depth + 1))
                    .or_insert(0);
            }
        }
    }

    for (_, node) in &mut reduced {
        node.depth = *tech_depth.get(&node.name.as_str()).unwrap_or(&0);
    }

    reduced
}

// milestone 格式: (technology name, is unlocked)，true表示解锁，false表示未解锁（就算是true，如果因为其他科技的false 导致无法解锁，也会被视为未解锁）
// 返回值：一系列可用科技名称。
pub fn resolve_dependency(data: &DataContext, milestones: &[(String, bool)]) -> Vec<String> {
    // 表示科技是否出现在里程碑中并且未解锁。未解锁状态会扩散到所有依赖它的科技上。

    fn appeared_in_milestones(tech_name: &str, milestones: &[(String, bool)]) -> bool {
        milestones
            .iter()
            .any(|(name, unlocked)| *name == tech_name && !*unlocked)
    }
    let mut unlocked = Dict::default();
    let mut queue = VecDeque::new(); // 传播队列
    for tech_name in data.technologies.keys() {
        unlocked.insert(
            tech_name.clone(),
            !appeared_in_milestones(tech_name, milestones),
        );
    }
    for (tech_name, unlocked) in milestones {
        if !*unlocked {
            queue.push_back(tech_name.clone());
        }
    }
    let mut visited = AIndexSet::default();
    while let Some(tech_name) = queue.pop_front() {
        if visited.contains(&tech_name) {
            continue;
        }
        visited.insert(tech_name.clone());
        if let Some(dependents) = data.technology_dependents.get(&tech_name) {
            for dependent in dependents {
                queue.push_back(dependent.clone());
                unlocked.insert(dependent.clone(), false);
            }
        }
    }
    unlocked
        .into_iter()
        .filter_map(|(name, is_unlocked)| if is_unlocked { Some(name) } else { None })
        .collect()
}

pub fn update_accessibles(user: &mut ProjectContext, data: &DataContext) {
    user.accessible_technologies = resolve_dependency(data, &user.tech_milestones);
    log::debug!("更新可访问科技: {:?}", user.milestone_graph);
    user.accessible_prototypes.clear();

    let mut new_recipe_productivity = user
        .accessible_technologies
        .iter()
        .filter_map(|tech_name| data.technologies.get(tech_name))
        .flat_map(|tech| &tech.effects)
        .filter_map(|effect| {
            if let Modifier::ChangeRecipeProductivity { recipe, change } = effect {
                Some((recipe.clone(), *change))
            } else {
                None
            }
        })
        .fold(AIndexMap::default(), |mut acc, (recipe, change)| {
            acc.entry(recipe)
                .and_modify(|c| *c += change)
                .or_insert(change);
            acc
        });
    // 将recipe_productivity和new_productivity取较大值，除非显式重置。
    for (recipe, change) in &mut user.recipe_productivity {
        if new_recipe_productivity.contains_key(recipe) {
            // 切换科技树后，如果对应的产能科技存在，保存最高的产能。
            new_recipe_productivity
                .entry(recipe.clone())
                .and_modify(|c| *c = (*c).max(*change))
                .or_insert(*change);
        }
    }
    user.recipe_productivity = new_recipe_productivity;
    user.recipe_productivity.sort_by(|ak, _, bk, _| ak.cmp(bk));

    let new_mining_productivity = user
        .accessible_technologies
        .iter()
        .filter_map(|tech_name| data.technologies.get(tech_name))
        .flat_map(|tech| &tech.effects)
        .filter_map(|effect| {
            if let Modifier::MiningDrillProductivityBonus(change) = effect {
                Some(change.modifier)
            } else {
                None
            }
        })
        .sum::<f64>();
    user.mining_productivity = new_mining_productivity.max(user.mining_productivity);

    for tech_name in &user.accessible_technologies {
        if let Some(tech) = data.technologies.get(tech_name) {
            for modifier in &tech.effects {
                match modifier {
                    Modifier::UnlockRecipe { recipe } => {
                        if let Some(_recipe_proto) = data.recipes.get(recipe) {
                            user.accessible_prototypes
                                .entry("recipe".to_string())
                                .or_default()
                                .insert(recipe.clone(), true);
                        }
                    }
                    Modifier::UnlockSpaceLocation { space_location } => {
                        user.accessible_prototypes
                            .entry("space-location".to_string())
                            .or_default()
                            .insert(space_location.clone(), true);
                    }
                    Modifier::UnlockQuality { quality } => {
                        user.accessible_prototypes
                            .entry("quality".to_string())
                            .or_default()
                            .insert(quality.clone(), true);
                    }
                    _ => {}
                }
            }
        }
    }

    for recipe in data.recipes.values() {
        if recipe.enabled {
            user.accessible_prototypes
                .entry("recipe".to_string())
                .or_default()
                .insert(recipe.base.name.clone(), true);
        }
        if !recipe.base.hidden
            && user
                .accessible_prototypes
                .get("recipe")
                .is_some_and(|recipes| recipes.contains_key(&recipe.base.name))
        {
            for result in &recipe.results {
                match result {
                    RecipeResult::Item(item) => {
                        user.accessible_prototypes
                            .entry("item".to_string())
                            .or_default()
                            .insert(item.name.clone(), true);
                    }
                    RecipeResult::Fluid(fluid) => {
                        user.accessible_prototypes
                            .entry("fluid".to_string())
                            .or_default()
                            .insert(fluid.name.clone(), true);
                    }
                }
            }
        }
    }

    for resource in data.resources.values() {
        if let Some(mining) = resource.base.minable.as_ref() {
            for result in &mining.results {
                match result {
                    RecipeResult::Item(item) => {
                        user.accessible_prototypes
                            .entry("item".to_string())
                            .or_default()
                            .insert(item.name.clone(), true);
                    }
                    RecipeResult::Fluid(fluid) => {
                        user.accessible_prototypes
                            .entry("fluid".to_string())
                            .or_default()
                            .insert(fluid.name.clone(), true);
                    }
                }
            }
            if let Some(result) = &mining.result {
                user.accessible_prototypes
                    .entry("item".to_string())
                    .or_default()
                    .insert(result.clone(), true);
            }
        }
    }

    for (item_name, item) in &data.items {
        if let Some(place_result) = &item.place_result
            && (user
                .accessible_prototypes
                .get("item")
                .is_some_and(|items| items.contains_key(item_name)))
        {
            // log::info!(
            //     "物品 {} 可放置，放置结果 {} 已解锁，添加到可访问原型中",
            //     item_name,
            //     place_result
            // );
            user.accessible_prototypes
                .entry("entity".to_string())
                .or_default()
                .insert(place_result.clone(), true);
        }
    }
    user.cur_max_quality_level = 0;
    for i in 1..data.qualities.len() {
        let quality = &data.qualities[i];
        if user
            .accessible_prototypes
            .get("quality")
            .is_some_and(|qualities| qualities.get(&quality.base.name).cloned().unwrap_or(false))
        {
            user.cur_max_quality_level = i as u8;
        } else {
            break;
        }
    }
    user.max_quality_level = (data.qualities.len() - 1) as u8;
}
