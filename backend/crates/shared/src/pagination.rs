use serde::{Deserialize, Serialize};

pub const DEFAULT_LIMIT: i64 = 25;
pub const MAX_LIMIT: i64 = 100;

#[derive(Debug, Clone, Deserialize)]
pub struct PageParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

impl PageParams {
    /// Page (>= 1) et limite bornée à `MAX_LIMIT`.
    pub fn normalized(&self) -> (i64, i64) {
        let page = self.page.unwrap_or(1).max(1);
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        (page, limit)
    }

    pub fn offset(&self) -> i64 {
        let (page, limit) = self.normalized();
        (page - 1) * limit
    }
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_is_capped() {
        let p = PageParams {
            page: Some(0),
            limit: Some(10_000),
        };
        assert_eq!(p.normalized(), (1, MAX_LIMIT));
        assert_eq!(p.offset(), 0);
    }

    #[test]
    fn offset_computed() {
        let p = PageParams {
            page: Some(3),
            limit: Some(10),
        };
        assert_eq!(p.offset(), 20);
    }
}
