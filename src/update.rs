const REPO_OWNER: &str = "cyx2015s";
const REPO_NAME: &str = "metatorio-calc";

pub fn update() {
    let release_builder = self_update::backends::github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()
        .unwrap();

    dbg!(release_builder.fetch().unwrap());
}

#[test]
fn test_update() {
    eprintln!("crate 版本是 {}", self_update::cargo_crate_version!());
    update();
}
