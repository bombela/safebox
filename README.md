#  SafeBox - less leaks of secrets in memory

[![Documentation](https://docs.rs/safebox/badge.svg)](https://docs.rs/safebox)

Via intrinsics, and guarding behind unsafe, the SafeBox helps you reduce the
chances of leaking some secret around in memory.

 - All read and write access must go through `get_ref` and `get_mut`. Both
   methods are marked `unsafe`. The idea is to prevent implicit copies. And
   hopefully get the programmer to be more careful on the sight of unsafe.
 - Memory is zeroed on Drop.
 - The API can surely be improved, PR and Issues for discussions are welcome.

**DO NOT USE IN PRODUCTION UNLESS YOU LIKE TO TRUST A RANDOM STRANGER ON THE INTERNET**

## Example

```rust
use safebox::SafeBox;
use rand::prelude::*;
let a = SafeBox::new_slice_with(8, &random::<u8>);
unsafe {
    println!("My secret: {:?}", secret.get_ref());
}
```

Prints (non-deterministic):
```text
My secret: [242, 144, 235, 196, 84, 35, 85, 232]
```

## Tidbits

To zero a piece RAM requires a bit more than calling memset(0). There is few layers of
optimizations and abstractions working against us:

 1) The compiler is free to forgo zeroing altogether. Since the value is never used again, the
    compiler assumes no side-effect, and will happily remove the code in the name of speed.
 2) The compiler is free to reorder memory operations.
 3) Similarly, the hardware is free to reorder memory operations.
 4) Furthermore, the hardware might not flush caches to RAM right away.

With the above taken care of, we must also enforce that the content cannot be copied in RAM
inadvertently:

 5) Taking ownership of the content can move it anywhere, including from/to the stack/heap.
    Leaving a stray copy of the content in RAM behind.
 6) Similarly, a &mut access on the content allows a mem swap or mem replace.

And finally, the Operating System can be involved:

 7) The Operating System can move the RAM to the SWAP area on persistent storage.
 8) Any thread in the same memory space can read & write the memory at will.
 9) When hibernating the OS will likely copy the RAM content to persistent storage.

This crate solves 1) to 4) with volatile write and atomic fence.
5) and 6) are guarded behind unsafe functions. Of course, the programmer is responsible to
   maintain the invariant; but at last; it is requires using a visible unsafe block.

The Operating System side is ignored. 7) and 8) could be addressed via mlock and mprotect
syscalls. And as for 9), you should use an encrypted storage anyway.
