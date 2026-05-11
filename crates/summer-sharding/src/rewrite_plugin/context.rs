#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRewritePair {
    pub logic: String,
    pub actual: String,
}

#[derive(Clone, Debug)]
pub struct ShardingRouteInfo {
    pub datasource: String,
    pub table_rewrites: Vec<TableRewritePair>,
    pub is_fanout: bool,
}

pub type RewriteContext<'a> = crate::sql_rewrite::SqlRewriteContext<'a>;
