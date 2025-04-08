# A prompt to generate document comments for Rust

Your goal is to generate document comments for existing Rust codes.

Requirements for changes:

* **Preserve existing code and attributes**: Do not modify, reorder, or remove any existing code, including function bodies, modules, or other definitions. Retain all attributes (`#[...]`) attached to items without modification or removal. Only add comments as specified.

* **Add documentation comments**: Add Rust-style documentation comments directly above each item definition, such as functions, modules, structs, constants, etc.

  * Use `///` for item-level comments and `//!` for module-level comments.
  * Follow the general documentation conventions used in Rust. Do not refer to JavaDoc or similar styles.
  * For `unsafe` functions, include a `# Safety` section. Clearly explain why the function is `unsafe` and what the caller must ensure.
    * This applies **only** to functions explicitly declared as `unsafe`, such as `unsafe fn (...) {...}` or `pub unsafe extern "C" fn (...) {...}`.
    * Functions that merely contain `unsafe` blocks within their body **do not** require a `# Safety` section.
    * Example:

      ```rust
      /// Performs a raw memory copy.
      ///
      /// # Safety
      ///
      /// The caller must ensure that `src` and `dst` are valid pointers and that
      /// the memory regions do not overlap.
      pub unsafe fn copy_memory(src: *const u8, dst: *mut u8, len: usize) {
          // SAFETY: The caller guarantees that `src` and `dst` are valid and non-overlapping.
          unsafe {
              std::ptr::copy_nonoverlapping(src, dst, len);
          }
      }
      ```

  * For `unsafe` blocks within function bodies, add a `// SAFETY` comment explaining why the block is safe to execute and what assumptions are being made.
    * Example:

      Inside `unsafe` elements:

      ```rust
      /// Converts a mutable string slice to a mutable byte slice.
      ///
      /// # Safety
      ///
      /// The caller must ensure that the content of the slice is valid UTF-8
      /// before the borrow ends and the underlying `str` is used.
      ///
      /// Use of a `str` whose contents are not valid UTF-8 is undefined behavior.
      ///
      /// ...
      pub unsafe fn as_bytes_mut(&mut self) -> &mut [u8] {
          // SAFETY: the cast from `&str` to `&[u8]` is safe since `str`
          // has the same layout as `&[u8]` (only libstd can make this guarantee).
          // The pointer dereference is safe since it comes from a mutable reference which
          // is guaranteed to be valid for writes.
          unsafe { &mut *(self as *mut str as *mut [u8]) }
      }
      ```

      Inside *safe* elements:

      ```rust
      pub fn split_at(&self, mid: usize) -> (&str, &str) {
          // is_char_boundary checks that the index is in [0, .len()]
          if self.is_char_boundary(mid) {
              // SAFETY: just checked that `mid` is on a char boundary.
              unsafe { (self.get_unchecked(0..mid), self.get_unchecked(mid..self.len())) }
          } else {
              slice_error_fail(self, 0, mid)
          }
      }
      ```

  * Avoid adding sections like `# Arguments` or `# Returns` unless absolutely necessary for clarity.
  * Use proper Markdown formatting. For example:
    * Leave a blank line after a section heading before starting a new paragraph.
    * Use code blocks for examples or code snippets.

* **Trait implementations**: Do not add comments for trait implementations (`impl Trait for Type`). Trait implementations are assumed to follow the documentation of the trait itself. However, add comments for inherent implementations (`impl Type`) and their functions.
  * Example:

    Do not add comments for trait implementation:

    ```rust
    impl Trait for MyStruct {
        fn do_something(&self) {
            println!("Doing something!");
        }
    }
    ```

    Add comments for inherent implementation:

    ```rust
    impl MyStruct {
        /// Creates a new instance of `MyStruct`.
        pub fn new() -> Self {
            Self {}
        }
    }
    ```

* **Module-level comments**: Add module-level comments at the top of each module. Use `//!`-style comments for module-level documentation. These comments should provide an overview of the module's purpose and functionality.
  * Example:

    ```rust
    //! This module provides utilities for working with strings.
    //!
    //! It includes functions for trimming, splitting, and joining strings.
    ```

    For more complex modules:

    ```rust
    //! This module implements a multi-threaded task scheduler.
    //!
    //! The scheduler allows tasks to be executed concurrently across multiple threads.
    //! It provides APIs for task creation, synchronization, and communication.
    //!
    //! # Examples
    //!
    //! ```
    //! use scheduler::TaskScheduler;
    //!
    //! let scheduler = TaskScheduler::new(4); // Create a scheduler with 4 threads.
    //! scheduler.spawn(|| {
    //!     println!("Task 1 is running");
    //! });
    //! scheduler.spawn(|| {
    //!     println!("Task 2 is running");
    //! });
    //! scheduler.run();
    //! ```
    ```

* **Clarity and conciseness**: Ensure the added comments are concise, clear, and relevant to the purpose of the item being documented. Avoid redundancy or overly verbose explanations.

* **Complex items**: If the item being documented is complex, provide a brief example or explanation to clarify its usage. Use code snippets where appropriate.
  * Example:

    ```rust
    /// A struct representing a 2D point.
    ///
    /// # Examples
    ///
    /// ```
    /// let point = Point { x: 1.0, y: 2.0 };
    /// println!("Point: ({}, {})", point.x, point.y);
    /// ```
    struct Point {
        x: f64,
        y: f64,
    }
    ```

* **Formatting**: Ensure all comments are properly formatted and adhere to Rust's documentation standards.

By following these guidelines, you will create high-quality documentation that is both informative and easy to understand.
