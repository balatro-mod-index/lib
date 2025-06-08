use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tree<'a> {
    pub hostname: &'a str,
    pub namespace: &'a str,
    pub name: &'a str,
    pub rev: &'a str,
    pub forge: Forge,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Forge {
    GitHub,
    GitLab,
}
impl Default for Tree<'_> {
    fn default() -> Self {
        Tree {
            hostname: "gitlab.com",
            namespace: "balatro-mod-index",
            name: "repo",
            rev: "main",
            forge: Forge::GitLab,
        }
    }
}
static DEFAULT_TREE: LazyLock<Tree> = LazyLock::new(Tree::default);
impl Default for &Tree<'_> {
    fn default() -> Self {
        &DEFAULT_TREE
    }
}
