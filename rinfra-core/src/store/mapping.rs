use crate::error::AppError;

use super::{DbRow, DbValue};

/// Construct an entity from a `DbRow`.
pub trait FromRow: Sized {
    fn from_row(row: &DbRow) -> Result<Self, AppError>;
}

/// Serialize an entity into insert/update parameters.
pub trait ToRow {
    /// The database table name this entity maps to.
    fn table_name() -> &'static str;

    /// Column names (excluding auto-generated ones like serial PKs).
    fn columns() -> &'static [&'static str];

    /// The primary key column name. Defaults to `"id"`.
    fn id_column() -> &'static str {
        "id"
    }

    /// Convert fields into (column_name, value) pairs for insert/update.
    fn to_params(&self) -> Vec<(&'static str, DbValue)>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Entity;

    #[derive(Debug, Clone, PartialEq)]
    struct TestUser {
        id: i64,
        name: String,
        email: String,
    }

    impl Entity for TestUser {
        type Id = i64;
        fn id(&self) -> &i64 {
            &self.id
        }
    }

    impl FromRow for TestUser {
        fn from_row(row: &DbRow) -> Result<Self, AppError> {
            Ok(Self {
                id: row.get("id")?,
                name: row.get("name")?,
                email: row.get("email")?,
            })
        }
    }

    impl ToRow for TestUser {
        fn table_name() -> &'static str {
            "users"
        }

        fn columns() -> &'static [&'static str] {
            &["name", "email"]
        }

        fn to_params(&self) -> Vec<(&'static str, DbValue)> {
            vec![
                ("name", DbValue::Text(self.name.clone())),
                ("email", DbValue::Text(self.email.clone())),
            ]
        }
    }

    #[test]
    fn test_from_row() {
        let row = DbRow::new(
            vec!["id".into(), "name".into(), "email".into()],
            vec![
                DbValue::Int(1),
                DbValue::Text("Alice".into()),
                DbValue::Text("alice@example.com".into()),
            ],
        );
        let user = TestUser::from_row(&row).unwrap();
        assert_eq!(user.id, 1);
        assert_eq!(user.name, "Alice");
        assert_eq!(user.email, "alice@example.com");
    }

    #[test]
    fn test_to_row() {
        let user = TestUser {
            id: 0,
            name: "Bob".into(),
            email: "bob@example.com".into(),
        };
        assert_eq!(TestUser::table_name(), "users");
        assert_eq!(TestUser::columns(), &["name", "email"]);
        assert_eq!(TestUser::id_column(), "id");
        let params = user.to_params();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "name");
        assert_eq!(params[1].0, "email");
    }
}
