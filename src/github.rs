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
            namespace: "kasimeka",
            name: "bmm-index-ng",
            rev: "main",
        }
    }
}
