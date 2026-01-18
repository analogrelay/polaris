# Instructions for Agents

This project is a hobby operating system written in Rust. It is intended to support multiple architectures in the future, but for now it primarily targets x86_64.

When working with this codebase, please keep the following points in mind:

* Use documentation comments for all items (public and non-public) to explain their purpose and usage.
* AVOID comments explaining what the code does; instead, focus on why it does it that way.
* Comments are NEVER to be used to provide commentary to the user or developer during code generation, nor are they to be used for thought processes.
* AVOID testing obvious things like constructors, display formatting, or basic getters/setters unless there is non-trivial logic involved.
* Isolate architecture-specific code into `arch` modules which conditionally import a child module based on the target architecture.
* Use devenv, and the devenv MCP tool to manage the development environment (installing mandatory packages, etc.).
* Add dependencies using `cargo add` rather than manually editing `Cargo.toml`.
* Use `just test` to run tests in the codebase using the correct arguments.
* Use `just check` to check code across the codebase quickly rather than checking individual crates.
* Use `just build` to build the entire codebase, which ensures all targets are built correctly.
* Use `just run -display none` to run the OS in QEMU without a graphical display, which is useful for debugging via serial output.
* Test functions should NEVER be prefixed with `test_`. They're already tests.
* When closed polymorphism is needed, prefer enums with variants over trait objects. For example:

```rust
enum MyEnum {
    VariantA(TypeA),
    VariantB(TypeB),
}

impl MyEnum {
    pub fn variant_a() -> Self {
        MyEnum::VariantA(TypeA::new())
    }

    pub fn variant_b() -> Self {
        MyEnum::VariantB(TypeB::new())
    }

    pub fn do_something(&self) {
        match self {
            MyEnum::VariantA(a) => a.do_something(),
            MyEnum::VariantB(b) => b.do_something(),
        }
    }
}

// Implement TypeA and TypeB with their respective methods.
```
