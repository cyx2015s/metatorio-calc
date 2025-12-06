#[allow(dead_code)]
pub(crate) fn are_categories_compatible(
    machine_categories: &Vec<String>,
    recipe_categories: &Vec<String>,
) -> bool {
    for cat in recipe_categories {
        if machine_categories.contains(cat) {
            return true;
        }
    }
    false
}
