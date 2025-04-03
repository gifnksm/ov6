# A prompt to generate document comments for Rust

Your goal is to generate document comments for existing Rust codes.

Requirements for changes:

* **Preserve existing code and attributes**: Do not modify, reorder, or remove any existing code, including function bodies, modules, or other definitions. Retain all attributes (`#[...]`) attached to items without modification or removal. Only add comments as specified.

* **Add documentation comments**: Add Rust-style documentation comments directly above each item definition, such as functions, modules, structs, constants, etc.

  * Use `///` for item-level comments and `//!` for module-level comments.
  * Follow the general documentation conventions used in Rust. Do not refer to JavaDoc or similar styles.
  * For `unsafe` functions, include a `# Safety` section. Clearly explain why the function is `unsafe` and what the caller must ensure.
  * Avoid adding sections like `# Arguments` or `# Returns` unless absolutely necessary for clarity.
  * Use proper Markdown formatting. For example:
    * Leave a blank line after a section heading before starting a new paragraph.
    * Use code blocks for examples or code snippets.

* **Trait implementations**: Do not add comments for trait implementations (`impl Trait for Type`). Trait implementations are assumed to follow the documentation of the trait itself. However, add comments for inherent implementations (`impl Type`) and their functions.

* **Module-level comments**: Add module-level comments at the top of each module. Use `//!`-style comments for module-level documentation. These comments should provide an overview of the module's purpose and functionality.

* **Clarity and conciseness**: Ensure the added comments are concise, clear, and relevant to the purpose of the item being documented. Avoid redundancy or overly verbose explanations.

* **Complex items**: If the item being documented is complex, provide a brief example or explanation to clarify its usage. Use code snippets where appropriate.

* **Formatting**: Ensure all comments are properly formatted and adhere to Rust's documentation standards.

By following these guidelines, you will create high-quality documentation that is both informative and easy to understand.
