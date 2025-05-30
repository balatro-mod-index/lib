pub struct Tree<'a> {
    pub hostname: &'a str,
    pub namespace: &'a str,
    pub name: &'a str,
    pub rev: &'a str,
}
impl Default for Tree<'_> {
    fn default() -> Self {
        Tree {
            hostname: "github.com",
            namespace: "balatro-mod-index",
            name: "repo",
            rev: "main",
        }
    }
}
