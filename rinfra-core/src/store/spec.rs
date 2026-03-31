use super::DbValue;

/// Declarative query specification that produces a WHERE clause.
pub trait Specification: Send + Sync {
    /// Generate `(where_clause, params)`.
    /// The clause should use `$N` placeholders starting at `$offset`.
    fn to_where_clause(&self, offset: usize) -> (String, Vec<DbValue>);
}

// ---------------------------------------------------------------------------
// Built-in combinators
// ---------------------------------------------------------------------------

/// AND combinator: `left AND right`.
pub struct AndSpec {
    left: Box<dyn Specification>,
    right: Box<dyn Specification>,
}

impl AndSpec {
    pub fn new(left: Box<dyn Specification>, right: Box<dyn Specification>) -> Self {
        Self { left, right }
    }
}

impl Specification for AndSpec {
    fn to_where_clause(&self, offset: usize) -> (String, Vec<DbValue>) {
        let (l_clause, l_params) = self.left.to_where_clause(offset);
        let (r_clause, r_params) = self.right.to_where_clause(offset + l_params.len());
        let clause = format!("({l_clause}) AND ({r_clause})");
        let mut params = l_params;
        params.extend(r_params);
        (clause, params)
    }
}

/// OR combinator: `left OR right`.
pub struct OrSpec {
    left: Box<dyn Specification>,
    right: Box<dyn Specification>,
}

impl OrSpec {
    pub fn new(left: Box<dyn Specification>, right: Box<dyn Specification>) -> Self {
        Self { left, right }
    }
}

impl Specification for OrSpec {
    fn to_where_clause(&self, offset: usize) -> (String, Vec<DbValue>) {
        let (l_clause, l_params) = self.left.to_where_clause(offset);
        let (r_clause, r_params) = self.right.to_where_clause(offset + l_params.len());
        let clause = format!("({l_clause}) OR ({r_clause})");
        let mut params = l_params;
        params.extend(r_params);
        (clause, params)
    }
}

/// Equality: `field = $N`.
pub struct EqSpec {
    pub field: String,
    pub value: DbValue,
}

impl EqSpec {
    pub fn new(field: impl Into<String>, value: DbValue) -> Self {
        Self {
            field: field.into(),
            value,
        }
    }
}

impl Specification for EqSpec {
    fn to_where_clause(&self, offset: usize) -> (String, Vec<DbValue>) {
        (format!("{} = ${}", self.field, offset), vec![self.value.clone()])
    }
}

/// LIKE pattern: `field LIKE $N`.
pub struct LikeSpec {
    pub field: String,
    pub pattern: String,
}

impl LikeSpec {
    pub fn new(field: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            pattern: pattern.into(),
        }
    }
}

impl Specification for LikeSpec {
    fn to_where_clause(&self, offset: usize) -> (String, Vec<DbValue>) {
        (
            format!("{} LIKE ${}", self.field, offset),
            vec![DbValue::Text(self.pattern.clone())],
        )
    }
}

/// IN list: `field IN ($N, $N+1, ...)`.
pub struct InSpec {
    pub field: String,
    pub values: Vec<DbValue>,
}

impl InSpec {
    pub fn new(field: impl Into<String>, values: Vec<DbValue>) -> Self {
        Self {
            field: field.into(),
            values,
        }
    }
}

impl Specification for InSpec {
    fn to_where_clause(&self, offset: usize) -> (String, Vec<DbValue>) {
        let placeholders: Vec<String> = (0..self.values.len())
            .map(|i| format!("${}", offset + i))
            .collect();
        (
            format!("{} IN ({})", self.field, placeholders.join(", ")),
            self.values.clone(),
        )
    }
}

/// BETWEEN: `field BETWEEN $N AND $N+1`.
pub struct BetweenSpec {
    pub field: String,
    pub low: DbValue,
    pub high: DbValue,
}

impl BetweenSpec {
    pub fn new(field: impl Into<String>, low: DbValue, high: DbValue) -> Self {
        Self {
            field: field.into(),
            low,
            high,
        }
    }
}

impl Specification for BetweenSpec {
    fn to_where_clause(&self, offset: usize) -> (String, Vec<DbValue>) {
        (
            format!("{} BETWEEN ${} AND ${}", self.field, offset, offset + 1),
            vec![self.low.clone(), self.high.clone()],
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eq_spec() {
        let spec = EqSpec::new("name", DbValue::Text("Alice".into()));
        let (clause, params) = spec.to_where_clause(1);
        assert_eq!(clause, "name = $1");
        assert_eq!(params, vec![DbValue::Text("Alice".into())]);
    }

    #[test]
    fn test_like_spec() {
        let spec = LikeSpec::new("name", "%ali%");
        let (clause, params) = spec.to_where_clause(1);
        assert_eq!(clause, "name LIKE $1");
        assert_eq!(params, vec![DbValue::Text("%ali%".into())]);
    }

    #[test]
    fn test_in_spec() {
        let spec = InSpec::new("id", vec![DbValue::Int(1), DbValue::Int(2), DbValue::Int(3)]);
        let (clause, params) = spec.to_where_clause(1);
        assert_eq!(clause, "id IN ($1, $2, $3)");
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_between_spec() {
        let spec = BetweenSpec::new("age", DbValue::Int(18), DbValue::Int(65));
        let (clause, params) = spec.to_where_clause(1);
        assert_eq!(clause, "age BETWEEN $1 AND $2");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_and_spec() {
        let left = Box::new(EqSpec::new("name", DbValue::Text("Alice".into())));
        let right = Box::new(EqSpec::new("age", DbValue::Int(30)));
        let spec = AndSpec::new(left, right);
        let (clause, params) = spec.to_where_clause(1);
        assert_eq!(clause, "(name = $1) AND (age = $2)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_or_spec() {
        let left = Box::new(EqSpec::new("status", DbValue::Text("active".into())));
        let right = Box::new(EqSpec::new("status", DbValue::Text("pending".into())));
        let spec = OrSpec::new(left, right);
        let (clause, params) = spec.to_where_clause(1);
        assert_eq!(clause, "(status = $1) OR (status = $2)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_nested_spec() {
        let eq = Box::new(EqSpec::new("active", DbValue::Bool(true)));
        let between = Box::new(BetweenSpec::new("age", DbValue::Int(20), DbValue::Int(50)));
        let and = AndSpec::new(eq, between);
        let (clause, params) = and.to_where_clause(1);
        assert_eq!(clause, "(active = $1) AND (age BETWEEN $2 AND $3)");
        assert_eq!(params.len(), 3);
    }
}
