use std::collections::{HashSet, VecDeque};

use crate::factorio::{
    DataContext, Dict, Modifier, ProjectContext, RecipeResult, TechnologyPrototype,
};

// milestone 格式: (technology name, is unlocked)，true表示解锁，false表示未解锁（就算是true，如果因为其他科技的false 导致无法解锁，也会被视为未解锁）
// 返回值：一系列可用科技名称。
pub fn resolve_dependency(
    techs: &Dict<TechnologyPrototype>,
    milestones: &[(String, bool)],
) -> Vec<String> {
    // 表示科技是否出现在里程碑中并且未解锁。未解锁状态会扩散到所有依赖它的科技上。

    fn appeared_in_milestones(tech_name: &str, milestones: &[(String, bool)]) -> bool {
        milestones
            .iter()
            .any(|(name, unlocked)| *name == tech_name && !*unlocked)
    }
    let mut dependents = Dict::<HashSet<String>>::new();
    let mut unlocked = Dict::new();
    let mut queue = VecDeque::new(); // 传播队列
    for (tech_name, tech) in techs {
        for prereq in &tech.prerequisites {
            dependents
                .entry(prereq.clone())
                .or_default()
                .insert(tech_name.clone());
        }
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
    while let Some(tech_name) = queue.pop_front() {
        if let Some(dependents) = dependents.get(&tech_name) {
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
    user.accessible_technologies = resolve_dependency(&data.technologies, &user.tech_milestones);
    user.accessible_prototypes.clear();
    user.recipe_productivity.clear();
    user.mining_productivity = 0.0;
    for tech_name in &user.accessible_technologies {
        if let Some(tech) = data.technologies.get(tech_name) {
            for modifier in &tech.effects {
                match modifier {
                    Modifier::UnlockRecipe { recipe } => {
                        if let Some(recipe_proto) = data.recipes.get(recipe) {
                            user.accessible_prototypes
                                .entry("recipe".to_string())
                                .or_default()
                                .insert(recipe.clone(), true);
                            for result in &recipe_proto.results {
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
                    Modifier::ChangeRecipeProductivity { recipe, change } => {
                        let entry = user
                            .recipe_productivity
                            .entry(recipe.clone())
                            .or_insert(0.0);
                        *entry += *change;
                    }
                    Modifier::MiningDrillProductivityBonus(modifier) => {
                        user.mining_productivity += modifier.modifier;
                    }
                    _ => {}
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
            && user
                .accessible_prototypes
                .get("item")
                .is_some_and(|items| items.contains_key(item_name))
        {
            user.accessible_prototypes
                .entry("entity".to_string())
                .or_default()
                .insert(place_result.clone(), true);
        }
    }
    for i in 1..data.qualities.len() {
        let quality = &data.qualities[i];
        if user
            .accessible_prototypes
            .get("quality")
            .is_none_or(|qualities| qualities.get(&quality.base.name).cloned().unwrap_or(false))
        {
            user.max_quality_level = i as u8;
        } else {
            break;
        }
    }
}
