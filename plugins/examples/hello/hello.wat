(module
  (memory (export "memory") 1)
  (func (export "greet") (result i32)
    i32.const 0)
  (data (i32.const 0) "hello from wasm plugin"))
