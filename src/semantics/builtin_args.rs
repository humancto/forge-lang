//! Argument errors of the common builtins, one text per builtin for both
//! engines (V3 finding 12: the engines had drifted apart, `join([1, 2], 5)`
//! even succeeded on the VM).
//!
//! Each engine still checks its own value representation, but both raise
//! [`bad_args`]`(name)` when a builtin's arguments have the wrong kinds, so
//! the text (and therefore the code, E0015, and the `(got ...)` annotation
//! added by `errors::annotate_builtin_error`) is the same. The shapes listed
//! here are what the builtins accept; `tests/parity/supported/
//! builtin_arg_errors.fg` calls each with wrong arguments on both engines.

/// What builtin `name` accepts, in the words of its argument error.
pub fn expected(name: &str) -> Option<&'static str> {
    Some(match name {
        "keys" | "values" => "an object or map",
        "entries" => "an object",
        "from_entries" => "an array of [key, value] pairs",
        "enumerate" | "pop" | "sort" | "sum" | "min_of" | "max_of" | "unique" | "flatten"
        | "shuffle" | "sample" => "an array",
        "float" => "a number or numeric string",
        "join" => "(array[, separator_string])",
        "split" => "(string, delimiter_string)",
        "push" => "(array, value)",
        "starts_with" => "(string, prefix_string)",
        "ends_with" => "(string, suffix_string)",
        "replace" => "(string, from_string, to_string)",
        "substring" => "(string, start[, end])",
        "index_of" => "(string, substring) or (array, value)",
        "last_index_of" | "count" => "(string, substring)",
        "pad_start" | "pad_end" => "(string, length[, pad_string])",
        "repeat_str" => "(string, count)",
        "zip" => "two arrays",
        "chunk" => "(array, size)",
        "slice" => "(array or string, start[, end])",
        "map" | "filter" | "find" | "flat_map" | "any" | "all" | "partition" | "group_by" => {
            "(array, function)"
        }
        "reduce" => "(array, initial, function)",
        "has_key" => "(object, key_string)",
        "get" => "(object, key[, default]) or (array, index[, default])",
        "pick" | "omit" => "(object, [field_names])",
        "merge" => "objects",
        _ => return None,
    })
}

/// The argument error of builtin `name`: `join() requires (array[,
/// separator_string])`. A name without an entry in [`expected`] is a bug
/// in the caller (debug builds panic); release builds say "valid
/// arguments".
pub fn bad_args(name: &str) -> String {
    let what = expected(name).unwrap_or_else(|| {
        debug_assert!(false, "BUG: no argument description for {name}()");
        "valid arguments"
    });
    format!("{}() requires {}", name, what)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argument_errors_are_e0015_and_get_annotated() {
        for name in [
            "join", "split", "keys", "push", "map", "get", "merge", "float",
        ] {
            let message = bad_args(name);
            assert_eq!(
                crate::semantics::errors::classify(&message).code,
                "E0015",
                "{message}"
            );
            assert!(
                crate::semantics::errors::annotate_builtin_error(name, &message, &["Int"])
                    .is_some(),
                "{message}"
            );
        }
    }

    #[test]
    fn every_described_builtin_is_registered() {
        for name in [
            "keys",
            "values",
            "entries",
            "from_entries",
            "enumerate",
            "pop",
            "sort",
            "sum",
            "min_of",
            "max_of",
            "unique",
            "flatten",
            "shuffle",
            "sample",
            "float",
            "join",
            "split",
            "push",
            "starts_with",
            "ends_with",
            "replace",
            "substring",
            "index_of",
            "last_index_of",
            "count",
            "pad_start",
            "pad_end",
            "repeat_str",
            "zip",
            "chunk",
            "slice",
            "map",
            "filter",
            "find",
            "flat_map",
            "any",
            "all",
            "partition",
            "group_by",
            "reduce",
            "has_key",
            "get",
            "pick",
            "omit",
            "merge",
        ] {
            assert!(expected(name).is_some(), "{name}");
            assert!(crate::builtins_registry::global(name).is_some(), "{name}");
        }
    }
}
