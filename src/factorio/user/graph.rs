use std::collections::{HashSet, VecDeque};

use crate::factorio::{Dict, TechnologyPrototype};

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
            !appeared_in_milestones(&tech_name, milestones),
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
