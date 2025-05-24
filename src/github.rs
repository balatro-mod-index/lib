pub struct GitHubTree<'a> {
    pub hostname: &'a str,
    pub namespace: &'a str,
    pub name: &'a str,
    pub rev: &'a str,
}
impl Default for GitHubTree<'_> {
    fn default() -> Self {
        GitHubTree {
            hostname: "github.com",
            namespace: "kasimeka",
            name: "bmm-index-ng",
            rev: "main",
        }
    }
}
