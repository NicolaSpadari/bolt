(comment) @comment

[
  (tag_name)
  (nesting_selector)
  (universal_selector)
] @tag

[
  "~"
  ">"
  "+"
  "-"
  "|"
  "*"
  "/"
  "="
  "^="
  "|="
  "~="
  "$="
  "*="
] @operator

[
  "and"
  "or"
  "not"
  "only"
] @keyword.operator

(id_name) @selector.id

(class_name) @selector.class

(namespace_name) @namespace

(namespace_selector
  (tag_name) @namespace
  "|")

(attribute_name) @attribute

(pseudo_element_selector
  "::"
  (tag_name) @selector.pseudo)

(pseudo_class_selector
  ":"
  (class_name) @selector.pseudo)

; BOLT: `.css` suffixes so themes can color CSS like VS Code (fall back to @property / @function).
[
  (feature_name)
  (property_name)
] @property.css

(function_name) @function.css

[
  (plain_value)
  (keyframes_name)
  (keyword_query)
] @constant.builtin

(attribute_selector
  (plain_value) @string)

(parenthesized_query
  (keyword_query) @property)

([
  (property_name)
  (plain_value)
] @variable
  (#match? @variable "^--"))

[
  "@media"
  "@import"
  "@charset"
  "@namespace"
  "@supports"
  "@keyframes"
  (at_keyword)
  (to)
  (from)
  (important)
] @keyword

(string_value) @string

(color_value) @string.special

[
  (integer_value)
  (float_value)
] @number

(unit) @type.unit

[
  ","
  ":"
  "."
  "::"
  ";"
] @punctuation.delimiter

(id_selector
  "#" @punctuation.delimiter)

[
  "{"
  ")"
  "("
  "}"
  "["
  "]"
] @punctuation.bracket
