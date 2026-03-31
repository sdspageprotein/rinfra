use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Meta};

/// Derive `Entity` for a struct. Mark the primary key field with `#[id]`.
///
/// ```ignore
/// #[derive(Entity)]
/// pub struct User {
///     #[id]
///     pub id: i64,
///     pub name: String,
/// }
/// ```
#[proc_macro_derive(Entity, attributes(id))]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => panic!("Entity derive only supports named fields"),
        },
        _ => panic!("Entity derive only supports structs"),
    };

    let id_field = fields
        .iter()
        .find(|f| f.attrs.iter().any(|a| a.path().is_ident("id")))
        .expect("Entity requires exactly one field annotated with #[id]");

    let id_ident = id_field.ident.as_ref().unwrap();
    let id_ty = &id_field.ty;

    let expanded = quote! {
        impl rinfra_core::store::Entity for #name {
            type Id = #id_ty;
            fn id(&self) -> &#id_ty {
                &self.#id_ident
            }
        }
    };

    TokenStream::from(expanded)
}

/// Derive `FromRow` for a struct. Fields are extracted by name from `DbRow`.
///
/// Use `#[column("db_col")]` to map a field to a different column name.
///
/// ```ignore
/// #[derive(FromRow)]
/// pub struct User {
///     pub id: i64,
///     pub name: String,
///     #[column("created_at")]
///     pub created: i64,
/// }
/// ```
#[proc_macro_derive(FromRow, attributes(column))]
pub fn derive_from_row(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => panic!("FromRow derive only supports named fields"),
        },
        _ => panic!("FromRow derive only supports structs"),
    };

    let field_extractions: Vec<_> = fields
        .iter()
        .map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let col_name = get_column_name(f);
            quote! {
                #ident: row.get(#col_name)?
            }
        })
        .collect();

    let expanded = quote! {
        impl rinfra_core::store::FromRow for #name {
            fn from_row(row: &rinfra_core::store::DbRow) -> Result<Self, rinfra_core::error::AppError> {
                Ok(Self {
                    #(#field_extractions,)*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

/// Derive `ToRow` for a struct. Use `#[table("name")]` on the struct.
/// Fields annotated with `#[id]` are excluded from `columns()` and `to_params()`.
/// Use `#[column("db_col")]` to map a field to a different column name.
///
/// ```ignore
/// #[derive(ToRow)]
/// #[table("users")]
/// pub struct User {
///     #[id]
///     pub id: i64,
///     pub name: String,
///     #[column("created_at")]
///     pub created: i64,
/// }
/// ```
#[proc_macro_derive(ToRow, attributes(table, column, id))]
pub fn derive_to_row(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let table_name = input
        .attrs
        .iter()
        .find_map(|attr| {
            if !attr.path().is_ident("table") {
                return None;
            }
            match &attr.meta {
                Meta::List(list) => {
                    let tokens = list.tokens.to_string();
                    let trimmed = tokens.trim_matches('"').to_string();
                    Some(trimmed)
                }
                _ => None,
            }
        })
        .expect("ToRow requires #[table(\"table_name\")] attribute on the struct");

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => panic!("ToRow derive only supports named fields"),
        },
        _ => panic!("ToRow derive only supports structs"),
    };

    let id_col_name = fields
        .iter()
        .find(|f| f.attrs.iter().any(|a| a.path().is_ident("id")))
        .map(|f| get_column_name(f))
        .unwrap_or_else(|| "id".to_string());

    let non_id_fields: Vec<_> = fields
        .iter()
        .filter(|f| !f.attrs.iter().any(|a| a.path().is_ident("id")))
        .collect();

    let col_names: Vec<String> = non_id_fields.iter().map(|f| get_column_name(f)).collect();

    let col_literals: Vec<_> = col_names
        .iter()
        .map(|c| {
            let lit = proc_macro2::Literal::string(c);
            quote! { #lit }
        })
        .collect();

    let param_entries: Vec<_> = non_id_fields
        .iter()
        .zip(col_names.iter())
        .map(|(f, col)| {
            let ident = f.ident.as_ref().unwrap();
            let col_lit = proc_macro2::Literal::string(col);
            quote! {
                (#col_lit, rinfra_core::store::IntoDbValue::into_db_value(self.#ident.clone()))
            }
        })
        .collect();

    let columns_count = col_literals.len();

    let expanded = quote! {
        impl rinfra_core::store::ToRow for #name {
            fn table_name() -> &'static str {
                #table_name
            }

            fn columns() -> &'static [&'static str] {
                static COLS: [&str; #columns_count] = [#(#col_literals),*];
                &COLS
            }

            fn id_column() -> &'static str {
                #id_col_name
            }

            fn to_params(&self) -> Vec<(&'static str, rinfra_core::store::DbValue)> {
                vec![#(#param_entries),*]
            }
        }
    };

    TokenStream::from(expanded)
}

fn get_column_name(f: &syn::Field) -> String {
    for attr in &f.attrs {
        if attr.path().is_ident("column") {
            if let Meta::List(list) = &attr.meta {
                let tokens = list.tokens.to_string();
                return tokens.trim_matches('"').to_string();
            }
        }
    }
    f.ident.as_ref().unwrap().to_string()
}
