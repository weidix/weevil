use super::*;

#[test]
fn dedupe_paths_keeps_first_order() {
    let deduped = dedupe_paths(vec![
        std::path::PathBuf::from("a.lua"),
        std::path::PathBuf::from("b.lua"),
        std::path::PathBuf::from("a.lua"),
    ]);
    assert_eq!(
        deduped,
        vec![
            std::path::PathBuf::from("a.lua"),
            std::path::PathBuf::from("b.lua")
        ]
    );
}
