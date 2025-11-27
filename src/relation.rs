pub fn are_categories_compatible(
    machine_categories: &Option<Vec<String>>,
    recipe_categories: &Option<Vec<String>>,
) -> bool {
    match (machine_categories, recipe_categories) {
        (Some(machine_cats), Some(recipe_cats)) => {
            for cat in recipe_cats {
                if machine_cats.contains(cat) {
                    return true;
                }
            }
            false
        }
        (None, None) => true,
        (Some(_), None) => false,
        (None, Some(_)) => false,
    }
}
