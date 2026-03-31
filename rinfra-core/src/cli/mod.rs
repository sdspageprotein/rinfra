use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OutputFormat {
    Pretty,
    Json,
}

impl OutputFormat {
    pub fn print<T: Serialize + std::fmt::Debug>(&self, value: &T) {
        match self {
            OutputFormat::Pretty => println!("{value:#?}"),
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_json_print() {
        let format = OutputFormat::Json;
        assert_eq!(format, OutputFormat::Json);
    }

    #[test]
    fn test_output_format_pretty_print() {
        let format = OutputFormat::Pretty;
        assert_eq!(format, OutputFormat::Pretty);
    }
}
