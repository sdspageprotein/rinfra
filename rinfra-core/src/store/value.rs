use crate::error::{AppError, ErrorCode};

/// Cross-database value type for parameter binding and result extraction.
#[derive(Debug, Clone, PartialEq)]
pub enum DbValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

impl DbValue {
    pub fn is_null(&self) -> bool {
        matches!(self, DbValue::Null)
    }
}

/// Convert a `DbValue` into a concrete Rust type.
pub trait FromDbValue: Sized {
    fn from_db_value(val: &DbValue) -> Result<Self, AppError>;
}

/// Convert a Rust type into a `DbValue`.
pub trait IntoDbValue {
    fn into_db_value(self) -> DbValue;
}

// ---------------------------------------------------------------------------
// FromDbValue implementations
// ---------------------------------------------------------------------------

impl FromDbValue for bool {
    fn from_db_value(val: &DbValue) -> Result<Self, AppError> {
        match val {
            DbValue::Bool(v) => Ok(*v),
            DbValue::Int(v) => Ok(*v != 0),
            _ => Err(type_error("bool", val)),
        }
    }
}

impl FromDbValue for i32 {
    fn from_db_value(val: &DbValue) -> Result<Self, AppError> {
        match val {
            DbValue::Int(v) => Ok(*v as i32),
            _ => Err(type_error("i32", val)),
        }
    }
}

impl FromDbValue for i64 {
    fn from_db_value(val: &DbValue) -> Result<Self, AppError> {
        match val {
            DbValue::Int(v) => Ok(*v),
            _ => Err(type_error("i64", val)),
        }
    }
}

impl FromDbValue for f64 {
    fn from_db_value(val: &DbValue) -> Result<Self, AppError> {
        match val {
            DbValue::Float(v) => Ok(*v),
            DbValue::Int(v) => Ok(*v as f64),
            _ => Err(type_error("f64", val)),
        }
    }
}

impl FromDbValue for String {
    fn from_db_value(val: &DbValue) -> Result<Self, AppError> {
        match val {
            DbValue::Text(v) => Ok(v.clone()),
            _ => Err(type_error("String", val)),
        }
    }
}

impl FromDbValue for Vec<u8> {
    fn from_db_value(val: &DbValue) -> Result<Self, AppError> {
        match val {
            DbValue::Bytes(v) => Ok(v.clone()),
            _ => Err(type_error("Vec<u8>", val)),
        }
    }
}

impl<T: FromDbValue> FromDbValue for Option<T> {
    fn from_db_value(val: &DbValue) -> Result<Self, AppError> {
        if val.is_null() {
            Ok(None)
        } else {
            T::from_db_value(val).map(Some)
        }
    }
}

// ---------------------------------------------------------------------------
// IntoDbValue implementations
// ---------------------------------------------------------------------------

impl IntoDbValue for bool {
    fn into_db_value(self) -> DbValue {
        DbValue::Bool(self)
    }
}

impl IntoDbValue for i32 {
    fn into_db_value(self) -> DbValue {
        DbValue::Int(self as i64)
    }
}

impl IntoDbValue for i64 {
    fn into_db_value(self) -> DbValue {
        DbValue::Int(self)
    }
}

impl IntoDbValue for f64 {
    fn into_db_value(self) -> DbValue {
        DbValue::Float(self)
    }
}

impl IntoDbValue for String {
    fn into_db_value(self) -> DbValue {
        DbValue::Text(self)
    }
}

impl IntoDbValue for &str {
    fn into_db_value(self) -> DbValue {
        DbValue::Text(self.to_string())
    }
}

impl IntoDbValue for Vec<u8> {
    fn into_db_value(self) -> DbValue {
        DbValue::Bytes(self)
    }
}

impl<T: IntoDbValue> IntoDbValue for Option<T> {
    fn into_db_value(self) -> DbValue {
        match self {
            Some(v) => v.into_db_value(),
            None => DbValue::Null,
        }
    }
}

fn type_error(expected: &str, got: &DbValue) -> AppError {
    AppError::new(
        ErrorCode::StoreQueryFailed,
        format!("expected {expected}, got {got:?}"),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_db_value_int() {
        let val = DbValue::Int(42);
        assert_eq!(i64::from_db_value(&val).unwrap(), 42);
        assert_eq!(i32::from_db_value(&val).unwrap(), 42);
        assert_eq!(f64::from_db_value(&val).unwrap(), 42.0);
    }

    #[test]
    fn test_from_db_value_text() {
        let val = DbValue::Text("hello".into());
        assert_eq!(String::from_db_value(&val).unwrap(), "hello");
    }

    #[test]
    fn test_from_db_value_bool() {
        assert!(bool::from_db_value(&DbValue::Bool(true)).unwrap());
        assert!(bool::from_db_value(&DbValue::Int(1)).unwrap());
        assert!(!bool::from_db_value(&DbValue::Int(0)).unwrap());
    }

    #[test]
    fn test_from_db_value_option_some() {
        let val = DbValue::Int(10);
        let result = Option::<i64>::from_db_value(&val).unwrap();
        assert_eq!(result, Some(10));
    }

    #[test]
    fn test_from_db_value_option_null() {
        let val = DbValue::Null;
        let result = Option::<i64>::from_db_value(&val).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_from_db_value_type_mismatch() {
        let val = DbValue::Text("not a number".into());
        assert!(i64::from_db_value(&val).is_err());
    }

    #[test]
    fn test_into_db_value() {
        assert_eq!(42i64.into_db_value(), DbValue::Int(42));
        assert_eq!(true.into_db_value(), DbValue::Bool(true));
        assert_eq!("hi".into_db_value(), DbValue::Text("hi".into()));
        assert_eq!(3.14f64.into_db_value(), DbValue::Float(3.14));
    }

    #[test]
    fn test_into_db_value_option() {
        let some: Option<i64> = Some(5);
        assert_eq!(some.into_db_value(), DbValue::Int(5));
        let none: Option<i64> = None;
        assert_eq!(none.into_db_value(), DbValue::Null);
    }

    #[test]
    fn test_db_value_is_null() {
        assert!(DbValue::Null.is_null());
        assert!(!DbValue::Int(0).is_null());
    }
}
