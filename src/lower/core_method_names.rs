//! Public instance methods of Ruby 3.4.11, sorted, generated from Ruby itself:
//! `Object.public_instance_methods`, and those of Hash, Array, String,
//! Symbol, Integer and Float that Object does not have.

pub(crate) const OBJECT_METHODS: &[&str] = &[
    "!", "!=", "!~", "<=>", "==", "===", "__id__", "__send__",
    "class", "clone", "define_singleton_method", "display", "dup", "enum_for", "eql?", "equal?",
    "extend", "freeze", "frozen?", "hash", "inspect", "instance_eval", "instance_exec", "instance_of?",
    "instance_variable_defined?", "instance_variable_get", "instance_variable_set", "instance_variables", "is_a?", "itself", "kind_of?", "method",
    "methods", "nil?", "object_id", "private_methods", "protected_methods", "public_method", "public_methods", "public_send",
    "remove_instance_variable", "respond_to?", "send", "singleton_class", "singleton_method", "singleton_methods", "tap", "then",
    "to_enum", "to_s", "yield_self",
];

pub(crate) const CORE_VALUE_METHODS: &[&str] = &[
    "%", "&", "*", "**", "+", "+@", "-", "-@",
    "/", "<", "<<", "<=", "=~", ">", ">=", ">>",
    "[]", "[]=", "^", "abs", "abs2", "all?", "allbits?", "angle",
    "any?", "anybits?", "append", "append_as_bytes", "arg", "ascii_only?", "assoc", "at",
    "b", "between?", "bit_length", "bsearch", "bsearch_index", "byteindex", "byterindex", "bytes",
    "bytesize", "byteslice", "bytesplice", "capitalize", "capitalize!", "casecmp", "casecmp?", "ceil",
    "ceildiv", "center", "chain", "chars", "chomp", "chomp!", "chop", "chop!",
    "chr", "chunk", "chunk_while", "clamp", "clear", "codepoints", "coerce", "collect",
    "collect!", "collect_concat", "combination", "compact", "compact!", "compare_by_identity", "compare_by_identity?", "concat",
    "conj", "conjugate", "count", "crypt", "cycle", "deconstruct", "deconstruct_keys", "dedup",
    "default", "default=", "default_proc", "default_proc=", "delete", "delete!", "delete_at", "delete_if",
    "delete_prefix", "delete_prefix!", "delete_suffix", "delete_suffix!", "denominator", "detect", "difference", "dig",
    "digits", "div", "divmod", "downcase", "downcase!", "downto", "drop", "drop_while",
    "dump", "each", "each_byte", "each_char", "each_codepoint", "each_cons", "each_entry", "each_grapheme_cluster",
    "each_index", "each_key", "each_line", "each_pair", "each_slice", "each_value", "each_with_index", "each_with_object",
    "empty?", "encode", "encode!", "encoding", "end_with?", "entries", "even?", "except",
    "fdiv", "fetch", "fetch_values", "fill", "filter", "filter!", "filter_map", "find",
    "find_all", "find_index", "finite?", "first", "flat_map", "flatten", "flatten!", "floor",
    "force_encoding", "gcd", "gcdlcm", "getbyte", "grapheme_clusters", "grep", "grep_v", "group_by",
    "gsub", "gsub!", "has_key?", "has_value?", "hex", "i", "id2name", "imag",
    "imaginary", "include?", "index", "infinite?", "inject", "insert", "integer?", "intern",
    "intersect?", "intersection", "invert", "join", "keep_if", "key", "key?", "keys",
    "last", "lazy", "lcm", "length", "lines", "ljust", "lstrip", "lstrip!",
    "magnitude", "map", "map!", "match", "match?", "max", "max_by", "member?",
    "merge", "merge!", "min", "min_by", "minmax", "minmax_by", "modulo", "name",
    "nan?", "negative?", "next", "next!", "next_float", "nobits?", "none?", "nonzero?",
    "numerator", "oct", "odd?", "one?", "ord", "pack", "partition", "permutation",
    "phase", "polar", "pop", "positive?", "pow", "pred", "prepend", "prev_float",
    "product", "push", "quo", "rassoc", "rationalize", "real", "real?", "rect",
    "rectangular", "reduce", "rehash", "reject", "reject!", "remainder", "repeated_combination", "repeated_permutation",
    "replace", "reverse", "reverse!", "reverse_each", "rindex", "rjust", "rotate", "rotate!",
    "round", "rpartition", "rstrip", "rstrip!", "sample", "scan", "scrub", "scrub!",
    "select", "select!", "setbyte", "shift", "shuffle", "shuffle!", "singleton_method_added", "size",
    "slice", "slice!", "slice_after", "slice_before", "slice_when", "sort", "sort!", "sort_by",
    "sort_by!", "split", "squeeze", "squeeze!", "start_with?", "step", "store", "strip",
    "strip!", "sub", "sub!", "succ", "succ!", "sum", "swapcase", "swapcase!",
    "take", "take_while", "tally", "times", "to_a", "to_ary", "to_c", "to_f",
    "to_h", "to_hash", "to_i", "to_int", "to_proc", "to_r", "to_set", "to_str",
    "to_sym", "tr", "tr!", "tr_s", "tr_s!", "transform_keys", "transform_keys!", "transform_values",
    "transform_values!", "transpose", "truncate", "undump", "unicode_normalize", "unicode_normalize!", "unicode_normalized?", "union",
    "uniq", "uniq!", "unpack", "unpack1", "unshift", "upcase", "upcase!", "update",
    "upto", "valid_encoding?", "value?", "values", "values_at", "zero?", "zip", "|",
    "~",
];

pub(crate) fn defined_by_every_object(name: &str) -> bool {
    OBJECT_METHODS.binary_search(&name).is_ok()
}

pub(crate) fn defined_by_a_core_value(name: &str) -> bool {
    CORE_VALUE_METHODS.binary_search(&name).is_ok()
}
