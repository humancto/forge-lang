# Grammar

Simplified EBNF grammar for the Forge language. This is a reference guide, not a formal specification. Optional elements are marked with `?`, repetition with `*`, and alternation with `|`.

## Program

```ebnf
program        = statement* EOF ;
```

## Statements

```ebnf
statement      = let_stmt
               | assign_stmt
               | fn_def
               | struct_def
               | interface_def
               | impl_block
               | type_def
               | if_stmt
               | match_stmt
               | when_stmt
               | for_stmt
               | while_stmt
               | loop_stmt
               | return_stmt
               | break_stmt
               | continue_stmt
               | try_catch
               | import_stmt
               | spawn_stmt
               | destructure_stmt
               | check_stmt
               | safe_block
               | timeout_block
               | retry_block
               | schedule_block
               | watch_block
               | prompt_def
               | decorator_stmt
               | yield_stmt
               | expression_stmt ;
```

### Variable Declarations

```ebnf
let_stmt       = ( "let" | "set" ) "mut"? IDENT ( ":" type_ann )? ( "=" | "to" ) expr NEWLINE ;
assign_stmt    = ( IDENT | field_access | index_expr ) ( "=" | "+=" | "-=" | "*=" | "/=" ) expr NEWLINE
               | "change" IDENT "to" expr NEWLINE ;
destructure_stmt = ( "let" | "unpack" ) destruct_pattern ( "=" | "from" ) expr NEWLINE ;
destruct_pattern = "{" IDENT ( "," IDENT )* "}"
                 | "[" IDENT ( "," IDENT )* ( "," "..." IDENT )? "]" ;
```

### Function Definitions

```ebnf
fn_def         = decorator* ( "fn" | "define" | "async" "fn" | "forge" ) IDENT "(" param_list? ")" ( "->" type_ann )? block ;
param_list     = param ( "," param )* ;
param          = IDENT ( ":" type_ann )? ( "=" expr )? ;
block          = "{" statement* "}" ;
```

### Struct / Thing Definitions

```ebnf
struct_def     = ( "struct" | "thing" ) IDENT "{" field_def* "}" ;
field_def      = IDENT ":" type_ann ( "=" expr )? NEWLINE ;
```

### Interface / Power Definitions

```ebnf
interface_def  = ( "interface" | "power" ) IDENT "{" method_sig* "}" ;
method_sig     = IDENT "(" param_list? ")" ( "->" type_ann )? NEWLINE ;
```

### Impl / Give Blocks

```ebnf
impl_block     = "impl" IDENT "{" fn_def* "}"
               | "give" IDENT "{" fn_def* "}"
               | "give" IDENT "the" "power" IDENT "{" fn_def* "}" ;
```

### Type Definitions (ADTs)

```ebnf
type_def       = "type" IDENT "=" variant ( "|" variant )* ;
variant        = IDENT ( "(" type_ann ( "," type_ann )* ")" )? ;
```

### Control Flow

```ebnf
if_stmt        = "if" expr block ( ( "else" | "otherwise" | "nah" ) ( if_stmt | block ) )? ;
match_stmt     = "match" expr "{" match_arm* "}" ;
match_arm      = pattern "->" ( expr | block ) ","? ;
pattern        = "_"
               | literal
               | IDENT
               | IDENT "(" pattern ( "," pattern )* ")" ;

when_stmt      = "when" expr "{" when_arm* "}" ;
when_arm       = ( comparison_op expr | "else" ) "->" expr ","? ;

for_stmt       = "for" IDENT ( "," IDENT )? "in" expr block
               | "for" "each" IDENT "in" expr block ;
while_stmt     = "while" expr block ;
loop_stmt      = "loop" block ;
return_stmt    = "return" expr? NEWLINE ;
break_stmt     = "break" NEWLINE ;
continue_stmt  = "continue" NEWLINE ;
yield_stmt     = ( "yield" | "emit" ) expr NEWLINE ;
```

### Error Handling

```ebnf
try_catch      = "try" block "catch" IDENT block ;
```

### Import

```ebnf
import_stmt    = "import" STRING
               | "from" STRING "import" IDENT ( "," IDENT )* ;
```

### Spawn

```ebnf
spawn_stmt     = "spawn" block ;
```

### Innovation Statements

```ebnf
check_stmt     = "check" expr ( "is" "not"? "empty" | "contains" expr | "between" expr "and" expr ) ;
safe_block     = "safe" block ;
timeout_block  = "timeout" expr "seconds" block ;
retry_block    = "retry" expr "times" block ;
schedule_block = "schedule" "every" expr ( "seconds" | "minutes" ) block ;
watch_block    = "watch" expr block ;
```

### Decorators

```ebnf
decorator      = "@" IDENT ( "(" decorator_args? ")" )? ;
decorator_args = decorator_arg ( "," decorator_arg )* ;
decorator_arg  = IDENT ":" expr
               | expr ;
```

### Prompt Definitions

```ebnf
prompt_def     = "prompt" IDENT "(" param_list? ")" "{" prompt_body "}" ;
prompt_body    = ( "system" ":" STRING )? "user" ":" STRING ( "returns" ":" STRING )? ;
```

## Expressions

```ebnf
expr           = or_expr ;
or_expr        = and_expr ( "||" and_expr )* ;
and_expr       = equality ( "&&" equality )* ;
equality       = comparison ( ( "==" | "!=" ) comparison )* ;
comparison     = addition ( ( "<" | ">" | "<=" | ">=" ) addition )* ;
addition       = multiplication ( ( "+" | "-" ) multiplication )* ;
multiplication = unary ( ( "*" | "/" | "%" ) unary )* ;
unary          = ( "!" | "-" ) unary | postfix ;
postfix        = primary ( call | index | field_access | "?" )* ;

call           = "(" arg_list? ")" ;
arg_list       = expr ( "," expr )* ;
index          = "[" expr "]" ;
field_access   = "." IDENT ;

primary        = INT | FLOAT | STRING | "true" | "false" | "null"
               | IDENT
               | "(" expr ")"
               | array_lit
               | object_lit
               | lambda
               | pipeline
               | must_expr
               | ask_expr
               | freeze_expr
               | spread_expr
               | await_expr
               | struct_init
               | where_filter
               | pipe_chain
               | block_expr ;

array_lit      = "[" ( expr ( "," expr )* ","? )? "]" ;
object_lit     = "{" ( field_init ( "," field_init )* ","? )? "}" ;
field_init     = ( IDENT | STRING ) ":" expr | IDENT ;
lambda         = "|" param_list? "|" ( expr | block ) ;
pipeline       = expr "|>" expr ;
must_expr      = "must" expr ;
ask_expr       = "ask" expr ;
freeze_expr    = "freeze" expr ;
spread_expr    = "..." expr ;
await_expr     = ( "await" | "hold" ) expr ;
struct_init    = IDENT "{" ( IDENT ":" expr ( "," IDENT ":" expr )* ","? )? "}" ;
where_filter   = expr "where" IDENT comparison_op expr ;
pipe_chain     = "from" expr ( "keep" expr | "sort" "by" IDENT | "take" expr )+ ;
block_expr     = "{" statement* expr "}" ;
```

## Types

```ebnf
type_ann       = simple_type
               | array_type
               | generic_type
               | function_type
               | optional_type ;

simple_type    = "Int" | "Float" | "String" | "Bool" | "Json" | IDENT ;
array_type     = "[" type_ann "]" ;
generic_type   = IDENT "<" type_ann ( "," type_ann )* ">" ;
function_type  = "fn" "(" ( type_ann ( "," type_ann )* )? ")" "->" type_ann ;
optional_type  = type_ann "?" ;
```

## Lexical Elements

```ebnf
IDENT          = ( ALPHA | "_" ) ( ALPHA | DIGIT | "_" )* ;
INT            = DIGIT+ ;
FLOAT          = DIGIT+ "." DIGIT+ ;
STRING         = '"' ( CHAR | ESCAPE | INTERP )* '"' ;
RAW_STRING     = '"""' CHAR* '"""' ;
INTERP         = "{" expr "}" ;
ESCAPE         = "\\" ( "n" | "t" | "r" | "\\" | '"' | "{" ) ;
NEWLINE        = "\n" | "\r\n" | ";" ;
COMMENT        = "//" CHAR* NEWLINE ;
```

## Operator Summary

| Precedence  | Operators         | Associativity |
| ----------- | ----------------- | ------------- |
| 1 (lowest)  | `\|\|`            | Left          |
| 2           | `&&`              | Left          |
| 3           | `==` `!=`         | Left          |
| 4           | `<` `>` `<=` `>=` | Left          |
| 5           | `+` `-`           | Left          |
| 6           | `*` `/` `%`       | Left          |
| 7           | `!` `-` (unary)   | Right         |
| 8           | `?` (postfix try) | Left          |
| 9 (highest) | `.` `[]` `()`     | Left          |

## Notes

- Newlines are significant. They terminate statements unless the line ends with an operator or open delimiter.
- Semicolons can be used as explicit statement terminators.
- Comments start with `//` and extend to the end of the line.
- String interpolation uses `{expr}` inside double-quoted strings. Raw strings (`"""..."""`) do not interpolate.
- The `has` keyword is not reserved. It is parsed contextually inside struct/thing bodies.
